use std::collections::HashMap;
use std::sync::Arc;

use apollo_encoder::{Field, InputField, InputObjectDef, InputValue, ObjectDef, Schema, Type_};
use swc::{config::ParseOptions, try_with_handler, Compiler};
use swc_common::{FileName, FilePathMapping, SourceMap};
use swc_ecmascript::ast::{
    BindingIdent, Decl, Expr, Module, ModuleItem, Stmt, TsArrayType, TsEntityName, TsFnParam,
    TsKeywordType, TsKeywordTypeKind, TsPropertySignature, TsType, TsTypeAnn, TsTypeElement,
    TsTypeParamInstantiation, TsTypeRef, TsUnionOrIntersectionType, TsUnionType,
};
use swc_ecmascript::ast::{Program, TsFnOrConstructorType, TsFnType};

use anyhow::{Context, Result};

pub fn generate_schema(prog: Module, manifest: HashMap<String, GraphQLKind>) -> Result<String> {
    let mut ctx = CodeGenCtx::new(manifest);
    ctx.parse(prog)?;
    Ok(ctx.finish())
}

#[derive(Clone)]
enum FieldKind {
    Input,
    Object,
}

#[derive(Debug)]
pub enum GraphQLKind {
    Object,
    Input,
    Enum,
}

impl GraphQLKind {
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0 => Some(GraphQLKind::Object),
            1 => Some(GraphQLKind::Input),
            2 => Some(GraphQLKind::Enum),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
enum ParsedField {
    Input(InputField),
    Object(Field),
}

impl ParsedField {
    pub fn input(self) -> Option<InputField> {
        match self {
            Self::Input(input) => Some(input),
            Self::Object(_) => None,
        }
    }

    pub fn object(self) -> Option<Field> {
        match self {
            Self::Input(_) => None,
            Self::Object(f) => Some(f),
        }
    }

    pub fn new(kind: FieldKind, name: String, type_: Type_) -> Self {
        match kind {
            FieldKind::Input => Self::Input(InputField::new(name, type_)),
            FieldKind::Object => Self::Object(Field::new(name, type_)),
        }
    }

    pub fn with_args(
        kind: FieldKind,
        name: String,
        type_: Type_,
        args: Vec<InputValue>,
    ) -> Option<Self> {
        if let Self::Object(mut field) = Self::new(kind, name, type_) {
            args.into_iter().for_each(|f| field.arg(f));
            Some(Self::Object(field))
        } else {
            None
        }
    }
}

struct CodeGenCtx {
    schema: Schema,
    manifest: HashMap<String, GraphQLKind>,

    /// True when we are parsing the inputs of a field with arguments
    parsing_inputs: bool,
}

impl CodeGenCtx {
    /// `manifest` is generated from the first pass in the Typescript compiler API code
    fn new(manifest: HashMap<String, GraphQLKind>) -> Self {
        let schema = Schema::new();
        println!("MANIFESET: {:?}", manifest);
        Self {
            schema,
            manifest,
            parsing_inputs: false,
        }
    }

    fn parse(&mut self, prog: Module) -> Result<()> {
        for item in prog.body {
            match item {
                ModuleItem::Stmt(stmt) => {
                    self.parse_statement(stmt)?;
                }
                ModuleItem::ModuleDecl(_) => {}
            }
        }
        Ok(())
    }

    fn parse_statement(&mut self, stmt: Stmt) -> Result<()> {
        match stmt {
            Stmt::Decl(Decl::TsTypeAlias(alias)) => {
                let ident = alias.id.sym.as_ref();
                {
                    match self.manifest.get(ident) {
                        Some(&GraphQLKind::Input) => {
                            let mut input_def = InputObjectDef::new(ident.to_string());
                            self.parse_typed_fields(FieldKind::Input, &alias.type_ann)?
                                .into_iter()
                                .for_each(|f| input_def.field(f.input().unwrap()));

                            self.schema.input(input_def);
                        }
                        Some(_) => {
                            let mut object_def = ObjectDef::new(ident.to_string());
                            self.parse_typed_fields(FieldKind::Object, &alias.type_ann)?
                                .into_iter()
                                .for_each(|f| object_def.field(f.object().unwrap()));

                            self.schema.object(object_def);
                        }
                        // Skip types not in the manifest
                        None => {}
                    }
                    Ok(())
                }
            }
            _ => todo!(),
        }
    }

    fn parse_typed_fields(
        &mut self,
        field_kind: FieldKind,
        type_ann: &TsType,
    ) -> Result<Vec<ParsedField>> {
        let mut fields: Vec<ParsedField> = Vec::new();
        match type_ann {
            TsType::TsTypeLit(lit) => {
                for member in &lit.members {
                    match member {
                        TsTypeElement::TsPropertySignature(prop_sig) => {
                            fields.push(self.parse_field(field_kind.clone(), prop_sig)?);
                        }
                        r => return Err(anyhow::anyhow!("Invalid property type: {:?}", r)),
                    }
                }
            }
            _ => todo!(),
        };

        Ok(fields)
    }

    fn parse_field(
        &mut self,
        kind: FieldKind,
        prop_sig: &TsPropertySignature,
    ) -> Result<ParsedField> {
        let key = match &*prop_sig.key {
            Expr::Ident(ident) => ident.sym.to_string(),
            _ => return Err(anyhow::anyhow!("Invalid property signature type")),
        };

        match self.parse_type(
            &key,
            &prop_sig.type_ann.as_ref().unwrap().type_ann,
            prop_sig.optional,
        )? {
            (ty, None) => Ok(ParsedField::new(kind, key, ty)),
            (ty, Some(args)) => match ParsedField::with_args(kind, key, ty, args) {
                None => Err(anyhow::anyhow!(
                    "Only ObjectDefs can contain input fields with args"
                )),
                Some(field) => Ok(field),
            },
        }
    }

    /// Returns the type of a GraphQL field, returning arguments if it has any.
    /// `field_name` is used to generate names for Inputs, and can be an empty string
    /// if you don't expect the Typescript type to be a function
    ///
    /// Important thing to note is that we only consider a sub-set of types, because
    /// the Typescript code is widened by the Typescript Compiler API before we receive it.
    fn parse_type(
        &mut self,
        field_name: &str,
        type_ann: &TsType,
        optional: bool,
    ) -> Result<(Type_, Option<Vec<InputValue>>)> {
        let (ty, args) = match type_ann {
            TsType::TsKeywordType(TsKeywordType { kind, .. }) => {
                (Self::parse_keyword_type(kind)?, None)
            }
            TsType::TsArrayType(TsArrayType { elem_type, .. }) => (
                Type_::List {
                    // TODO: There is no way to set non-nullable array elements in TS,
                    // meaning we cant represent [Int!]!
                    ty: Box::new(self.parse_type(field_name, elem_type, true)?.0),
                },
                None,
            ),
            TsType::TsTypeRef(TsTypeRef {
                type_name,
                type_params,
                ..
            }) => {
                if let TsEntityName::Ident(ident) = type_name {
                    if ident.sym.to_string() != "Promise" {
                        match self.manifest.get(ident.sym.as_ref()) {
                            Some(&GraphQLKind::Object) if self.parsing_inputs => {
                                return Err(anyhow::anyhow!(
                                    "Field args can only be Inputs (check: {})",
                                    ident.sym.as_ref()
                                ));
                            }
                            Some(&GraphQLKind::Input) if !self.parsing_inputs => {
                                return Err(anyhow::anyhow!(
                                    "Field type can't be an Input (check: {})",
                                    ident.sym.as_ref()
                                ));
                            }
                            None | Some(_) => (
                                Type_::NamedType {
                                    name: ident.sym.to_string(),
                                },
                                None,
                            ),
                        }
                    } else {
                        match type_params {
                            None => {
                                return Err(anyhow::anyhow!("Missing type parameter for Promise"))
                            }
                            Some(TsTypeParamInstantiation { params, .. }) => {
                                match params.len() {
                                    1 => {}
                                    other => {
                                        return Err(anyhow::anyhow!(
                                            "Invalid amount of type parameters for Promise: {}",
                                            other
                                        ))
                                    }
                                }
                                let typ = &params[0];

                                // Somewhat confusing, but if we are here then we are parsing return of
                                // a field with arguments, meaning we don't know the optionality of the
                                // return type until we unwrap it from the Promise, meaning we should
                                // discard the `optional` param and return here
                                //
                                // Maybe we should move this match branch into its own dedicated function,
                                // and when we parse the return we call that instead of this function.
                                match &**typ {
                                    TsType::TsUnionOrIntersectionType(
                                        TsUnionOrIntersectionType::TsUnionType(u),
                                    ) if Self::is_nullable_union(typ) => {
                                        let non_null = Self::unwrap_union(u)?;
                                        return self.parse_type("", non_null, true);
                                    }
                                    _ => return self.parse_type("", typ, false),
                                }
                            }
                        }
                    }
                } else {
                    todo!()
                }
            }
            TsType::TsFnOrConstructorType(TsFnOrConstructorType::TsFnType(TsFnType {
                params,
                // `type_ann` here is return type
                type_ann,
                ..
            })) => {
                if params.len() != 1 {
                    return Err(anyhow::anyhow!("Expected only one parameter for field arg"));
                }

                let input = &params[0];

                let lit = match input {
                    TsFnParam::Ident(BindingIdent { type_ann, .. }) => {
                        if let Some(TsTypeAnn { type_ann, .. }) = type_ann {
                            match **type_ann {
                                TsType::TsTypeLit(ref lit) => Some(lit),
                                _ => None,
                            }
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                let lit = match lit {
                    None => return Err(anyhow::anyhow!("Type of Field args can only be objects")),
                    Some(lit) => lit,
                };

                let member_count = lit.members.len();
                self.parsing_inputs = true;
                let args = lit
                    .members
                    .iter()
                    .map(|f| self.parse_arg_member(field_name, f, member_count))
                    .collect::<Result<Vec<InputValue>>>()?;
                self.parsing_inputs = false;

                // Last param can be anything here, since we don't know if the return type is
                // optional until we parse it. `self.parse_type()` will make sure to return
                // the correct type if we are parsing return type
                let (ret_ty, _) = self.parse_type(field_name, &type_ann.type_ann, true)?;

                return Ok((ret_ty, Some(args)));
            }
            TsType::TsUnionOrIntersectionType(TsUnionOrIntersectionType::TsUnionType(uni)) => {
                let typ = Self::unwrap_union(uni)?;
                return self.parse_type(field_name, typ, true);
            }
            r => {
                println!("{:?}", r);
                todo!();
            }
        };

        if !optional {
            return Ok((Type_::NonNull { ty: Box::new(ty) }, args));
        }

        Ok((ty, args))
    }

    fn parse_arg_member(
        &mut self,
        field_name: &str,
        member: &TsTypeElement,
        member_count: usize,
    ) -> Result<InputValue> {
        match member {
            TsTypeElement::TsPropertySignature(prop_sig) => {
                let ident = match &*prop_sig.key {
                    Expr::Ident(ident) => ident,
                    _ => todo!(),
                };

                let type_ann = match &prop_sig.type_ann {
                    Some(t) => t,
                    None => return Err(anyhow::anyhow!("Missing property")),
                };

                let name = ident.sym.as_ref();

                let type_ = match &*type_ann.type_ann {
                    TsType::TsTypeLit(_) => {
                        let input_name = Self::compute_input_name(field_name, name, member_count);
                        let mut input_def = InputObjectDef::new(input_name.clone());

                        self.parse_typed_fields(FieldKind::Input, &type_ann.type_ann)?
                            .into_iter()
                            .for_each(|f| input_def.field(f.input().unwrap()));

                        self.schema.input(input_def);

                        if !prop_sig.optional {
                            Type_::NonNull {
                                ty: Box::new(Type_::NamedType { name: input_name }),
                            }
                        } else {
                            Type_::NamedType { name: input_name }
                        }
                    }
                    ty => {
                        let (ty, _) = self.parse_type(name, ty, prop_sig.optional)?;
                        ty
                    }
                };

                Ok(InputValue::new(ident.sym.to_string(), type_))
            }
            _ => Err(anyhow::anyhow!(
                "Field args input can only contain properties"
            )),
        }
    }

    fn finish(self) -> String {
        self.schema.finish()
    }
}

impl CodeGenCtx {
    fn is_nullable(ty: &TsType) -> bool {
        match ty {
            TsType::TsUnionOrIntersectionType(TsUnionOrIntersectionType::TsUnionType(_)) => {
                Self::is_nullable_union(ty)
            }
            TsType::TsKeywordType(TsKeywordType { kind, .. }) => matches!(
                kind,
                TsKeywordTypeKind::TsNullKeyword | TsKeywordTypeKind::TsUndefinedKeyword
            ),
            // For now just assume type references and others are non-null
            _ => false,
        }
    }

    /// Return true if type is like: `T | null or T | undefined`
    fn is_nullable_union(ty: &TsType) -> bool {
        match ty {
            TsType::TsUnionOrIntersectionType(TsUnionOrIntersectionType::TsUnionType(
                TsUnionType { types, .. },
            )) => types.iter().any(|ty| Self::is_nullable(ty)),
            _ => false,
        }
    }

    /// Return the non-nullable type of a union. This will error if there are more than 2
    /// types, or there is no nullable type present
    /// ```
    /// Ex: "User | null"          -> User
    ///     "User | string | null" -> Error
    ///     "User | string"        -> Error
    /// ```
    fn unwrap_union(ty: &TsUnionType) -> Result<&TsType> {
        if ty.types.len() != 2 {
            return Err(anyhow::anyhow!("Union types cannot have more than 2 types"));
        }

        let mut ret_ty: Option<&TsType> = None;
        let mut has_nullable = false;
        for typ in &ty.types {
            if Self::is_nullable(typ) {
                has_nullable = true;
            } else {
                ret_ty = Some(typ);
            }
        }

        if !has_nullable {
            return Err(anyhow::anyhow!(
                "Union types cannot have more than 2 non-nullable types"
            ));
        }

        if let Some(ret_ty) = ret_ty {
            Ok(ret_ty)
        } else {
            Err(anyhow::anyhow!("No non-nullable type found in union"))
        }
    }

    /// Computes a name for a new Input type. The resulting name depends on the value of
    /// `member_count`. If `member_count === 1`, then the return is simply a concatenation of
    ///  of `field_name` and the string "Input".
    ///
    /// Otherwise, we also concatenate the name of the param
    fn compute_input_name(field_name: &str, param_name: &str, member_count: usize) -> String {
        if member_count == 1 {
            format!(
                "{}{}",
                field_name
                    .chars()
                    .next()
                    .iter()
                    .map(|c| c.to_ascii_uppercase())
                    .chain(field_name.chars().skip(1))
                    .collect::<String>(),
                "Input",
            )
        } else {
            format!(
                "{}{}{}",
                field_name
                    .chars()
                    .next()
                    .iter()
                    .map(|c| c.to_ascii_uppercase())
                    .chain(field_name.chars().skip(1))
                    .collect::<String>(),
                "Input",
                param_name
                    .chars()
                    .next()
                    .iter()
                    .map(|c| c.to_ascii_uppercase())
                    .chain(param_name.chars().skip(1))
                    .collect::<String>(),
            )
        }
    }

    fn parse_keyword_type(kind: &TsKeywordTypeKind) -> Result<Type_> {
        match kind {
            TsKeywordTypeKind::TsNumberKeyword => Ok(Type_::NamedType { name: "Int".into() }),
            TsKeywordTypeKind::TsStringKeyword => Ok(Type_::NamedType {
                name: "String".into(),
            }),
            TsKeywordTypeKind::TsBooleanKeyword => Ok(Type_::NamedType {
                name: "Boolean".into(),
            }),
            // TODO: Scalar types like BigInt
            _ => todo!(),
        }
    }
}

pub fn parse_ts(s: &str, opts: &str) -> Result<Program> {
    let cm = Arc::new(SourceMap::new(FilePathMapping::empty()));
    let c = Arc::new(Compiler::new(cm));

    try_with_handler(c.cm.clone(), |handler| {
        let opts: ParseOptions = serde_json::from_str(opts).unwrap();

        let fm = c.cm.new_source_file(FileName::Anon, s.into());
        let program = c
            .parse_js(
                fm,
                handler,
                opts.target,
                opts.syntax,
                opts.is_module,
                opts.comments,
            )
            .context("failed to parse code")?;

        Ok(program)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    fn get_prog(src: &str) -> Program {
        parse_ts(
            src,
            "{
                \"syntax\": \"typescript\",
                \"tsx\": true,
                \"decorators\": false,
                \"dynamicImport\": false
          }",
        )
        .unwrap()
    }

    fn test(src: &str, expected: &str, mani: Vec<(&str, GraphQLKind)>) {
        let prog = get_prog(src);

        let mut map: HashMap<String, GraphQLKind> = HashMap::new();
        mani.into_iter().for_each(|(k, v)| {
            map.insert(k.into(), v);
        });

        let mut gen = CodeGenCtx::new(map);

        gen.parse(prog.module().unwrap()).unwrap();
        let out = gen.finish();
        println!("{}", out);
        assert_eq!(expected, out);
    }

    fn test_expect_err(src: &str, mani: Vec<(&str, GraphQLKind)>) {
        let prog = get_prog(src);
        let mut map: HashMap<String, GraphQLKind> = HashMap::new();
        mani.into_iter().for_each(|(k, v)| {
            map.insert(k.into(), v);
        });
        let mut gen = CodeGenCtx::new(map);
        match gen.parse(prog.module().unwrap()) {
            Err(_) => {}
            Ok(_) => {
                println!("Output: {}", gen.finish());
                panic!("Expected error")
            }
        }
    }

    #[test]
    fn it_parses_field_basic_types() {
        let src = "
        type User = { id: string; name: string; karma: number; active: boolean; }
        type Player = { user: User; level: number; }
        ";
        test(
            src,
            indoc! { r#"
            type User {
              id: String!
              name: String!
              karma: Int!
              active: Boolean!
            }
            type Player {
              user: User!
              level: Int!
            }
            "# },
            vec![
                ("User", GraphQLKind::Object),
                ("Player", GraphQLKind::Object),
            ],
        );

        // Optionals
        let src = "
        type User = { id?: string; name?: string; karma?: number; }
        type Player = { user: User; level?: number; }
        ";

        test(
            src,
            indoc! { r#"
            type User {
              id: String
              name: String
              karma: Int
            }
            type Player {
              user: User!
              level: Int
            }
            "# },
            vec![
                ("User", GraphQLKind::Object),
                ("Player", GraphQLKind::Object),
            ],
        );
    }

    #[test]
    fn it_fails_when_a_field_is_an_input() {
        // Basic
        let src = "
                type User = { id: string; name: string; karma: number; }
                type AnInput = { dummy: string }
                type Player = { user: User[]; woops: AnInput; }
                ";
        test_expect_err(
            src,
            vec![
                ("User", GraphQLKind::Object),
                ("AnInput", GraphQLKind::Input),
                ("Player", GraphQLKind::Object),
            ],
        );

        // Field with args
        let src = "
                type AnInput = { id: string; name: string; karma: number; }
                type Query = { doSomething: (input: { whatever: string }) => Promise<AnInput> }
                ";
        test_expect_err(
            src,
            vec![
                ("AnInput", GraphQLKind::Input),
                ("Query", GraphQLKind::Object),
            ],
        );
    }

    #[test]
    fn it_parses_array_fields() {
        // Basic
        let src = "
        type User = { id: string; name: string; karma: number; }
        type Player = { user: User[]; level: number[]; }
        ";
        test(
            src,
            indoc! { r#"
            type User {
              id: String!
              name: String!
              karma: Int!
            }
            type Player {
              user: [User]!
              level: [Int]!
            }
            "# },
            vec![
                ("User", GraphQLKind::Object),
                ("Player", GraphQLKind::Object),
            ],
        );

        // Optional
        let src = "
        type User = { id: string; name: string; karma: number; }
        type Player = { user?: User[]; level?: number[]; }
        ";
        test(
            src,
            indoc! { r#"
            type User {
              id: String!
              name: String!
              karma: Int!
            }
            type Player {
              user: [User]
              level: [Int]
            }
            "# },
            vec![
                ("User", GraphQLKind::Object),
                ("Player", GraphQLKind::Object),
            ],
        );

        // Nested
        let src = "
        type User = { id: string[][]; name: string; karma: number; }
        type Player = { user?: User[][]; level?: number[][]; }
        ";
        test(
            src,
            indoc! { r#"
            type User {
              id: [[String]]!
              name: String!
              karma: Int!
            }
            type Player {
              user: [[User]]
              level: [[Int]]
            }
            "# },
            vec![
                ("User", GraphQLKind::Object),
                ("Player", GraphQLKind::Object),
            ],
        );
    }

    #[cfg(test)]
    mod args_tests {
        use super::*;

        #[test]
        fn it_parses_fields_with_args() {
            // Basic
            let src = "
        type User = { id: string; name: string; karma: number; }
        type Query = { findUser: (args: { id?: string, name?: string, test: string[] }) => Promise<User[]>; }
        ";
            test(
                src,
                indoc! { r#"
            type User {
              id: String!
              name: String!
              karma: Int!
            }
            type Query {
              findUser(id: String, name: String, test: [String]!): [User]!
            }
            "# },
                vec![
                    ("User", GraphQLKind::Object),
                    ("Query", GraphQLKind::Object),
                ],
            );

            // With pre-defined input
            let src = "
        type User = { id: string; name: string; karma: number; }
        type FindUserInput = { name: string, id?: string }
        type Query = { findUser: (args: { input: FindUserInput }) => Promise<User | null>; }
        ";
            test(
                src,
                indoc! { r#"
            type User {
              id: String!
              name: String!
              karma: Int!
            }
            input FindUserInput {
              name: String!
              id: String
            }
            type Query {
              findUser(input: FindUserInput!): User
            }
            "# },
                vec![
                    ("User", GraphQLKind::Object),
                    ("FindUserInput", GraphQLKind::Input),
                    ("Query", GraphQLKind::Object),
                ],
            );

            // Mix and match
            let src = "
        type User = { id: string; name: string; karma: number; }
        type FindUserInput = { name: string, id?: string }
        type Query = { findUser: (args: { input: FindUserInput, karma?: number }) => Promise<User>; }
        ";
            test(
                src,
                indoc! { r#"
            type User {
              id: String!
              name: String!
              karma: Int!
            }
            input FindUserInput {
              name: String!
              id: String
            }
            type Query {
              findUser(input: FindUserInput!, karma: Int): User!
            }
            "# },
                vec![
                    ("User", GraphQLKind::Object),
                    ("FindUserInput", GraphQLKind::Input),
                    ("Query", GraphQLKind::Object),
                ],
            );
        }

        #[test]
        fn it_should_fail_when_given_multiple_args() {
            // Inlined type literal
            let src = "
        type User = { id: string; name: string; karma: number; }
        type Query = { findUser: (args: { name: string }, woops: { karma: number }) => Promise<User>; }
        ";
            test_expect_err(
                src,
                vec![
                    ("User", GraphQLKind::Object),
                    ("Query", GraphQLKind::Object),
                ],
            );
        }

        #[test]
        fn it_parses_type_literal_args() {
            let src = "
        type User = { id: string; name: string; karma: number; }
        type Query = { findUser: (args: { user: { name?: string, karma?: number } }) => Promise<User>; }
        ";
            test(
                src,
                indoc! { r#"
            type User {
              id: String!
              name: String!
              karma: Int!
            }
            input FindUserInput {
              name: String
              karma: Int
            }
            type Query {
              findUser(user: FindUserInput!): User!
            }
            "# },
                vec![
                    ("User", GraphQLKind::Object),
                    ("Query", GraphQLKind::Object),
                ],
            );
        }

        #[test]
        fn it_parses_type_literal_and_keyword_ags() {
            let src = "
            type User = { id: string; name: string; karma: number; }
            type Query = { findUser: (args: { user: { name?: string, karma?: number }, karma?: number }) => Promise<User>; }
            ";
            test(
                src,
                indoc! { r#"
                type User {
                  id: String!
                  name: String!
                  karma: Int!
                }
                input FindUserInputUser {
                  name: String
                  karma: Int
                }
                type Query {
                  findUser(user: FindUserInputUser!, karma: Int): User!
                }
                "# },
                vec![
                    ("User", GraphQLKind::Object),
                    ("Query", GraphQLKind::Object),
                ],
            );
        }

        #[test]
        fn it_parses_multiple_type_literal_args() {
            let src = "
            type User = { id: string; name: string; karma: number; }
            type Query = { findUser: (args: { user: { name?: string, karma?: number }, other?: { id: string } }) => Promise<User>; }
            ";
            test(
                src,
                indoc! { r#"
                type User {
                  id: String!
                  name: String!
                  karma: Int!
                }
                input FindUserInputUser {
                  name: String
                  karma: Int
                }
                input FindUserInputOther {
                  id: String!
                }
                type Query {
                  findUser(user: FindUserInputUser!, other: FindUserInputOther): User!
                }
                "# },
                vec![
                    ("User", GraphQLKind::Object),
                    ("Query", GraphQLKind::Object),
                ],
            );
        }

        #[test]
        fn it_should_fail_when_an_arg_isnt_an_input() {
            let src = "
        type User = { id: string; name: string; karma: number; }
        type NotAnInput = { id: string }
        type Query = { findUser: (args: { input: NotAnInput }) => Promise<User>; }
        ";
            test_expect_err(
                src,
                vec![
                    ("User", GraphQLKind::Object),
                    ("NotAnInput", GraphQLKind::Object),
                    ("Query", GraphQLKind::Object),
                ],
            );
        }

        #[test]
        fn it_should_identify_promised_return_type() {
            // Optionals
            let src = "
        type User = { id: string; name: string; karma: number; }
        type Query = { findUser: (args: { id: string }) => Promise<User | null>; }
        ";
            test(
                src,
                indoc! { r#"
                type User {
                  id: String!
                  name: String!
                  karma: Int!
                }
                type Query {
                  findUser(id: String!): User
                }
                "# },
                vec![
                    ("User", GraphQLKind::Object),
                    ("Query", GraphQLKind::Object),
                ],
            );
            let src = "
        type User = { id: string; name: string; karma: number; }
        type Query = { findUser: (args: { id: string }) => Promise<User | undefined>; }
        ";
            test(
                src,
                indoc! { r#"
                type User {
                  id: String!
                  name: String!
                  karma: Int!
                }
                type Query {
                  findUser(id: String!): User
                }
                "# },
                vec![
                    ("User", GraphQLKind::Object),
                    ("Query", GraphQLKind::Object),
                ],
            );
            let src = "
        type User = { id: string; name: string; karma: number; }
        type Query = { findUser: (args: { id: string }) => Promise<string>; }
        ";
            test(
                src,
                indoc! { r#"
                type User {
                  id: String!
                  name: String!
                  karma: Int!
                }
                type Query {
                  findUser(id: String!): String!
                }
                "# },
                vec![
                    ("User", GraphQLKind::Object),
                    ("Query", GraphQLKind::Object),
                ],
            );
        }
    }
}
