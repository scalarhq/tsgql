use std::sync::Arc;

use apollo_encoder::{Field, InputField, InputObjectDef, InputValue, ObjectDef, Schema, Type_};
use swc::{config::ParseOptions, try_with_handler, Compiler};
use swc_common::{FileName, FilePathMapping, SourceMap};
use swc_ecmascript::ast::{
    Decl, Expr, Module, ModuleItem, Stmt, TsArrayType, TsEntityName, TsFnParam, TsKeywordType,
    TsKeywordTypeKind, TsPropertySignature, TsType, TsTypeElement, TsTypeParamInstantiation,
    TsTypeRef, TsUnionOrIntersectionType, TsUnionType,
};
use swc_ecmascript::ast::{Program, TsFnOrConstructorType, TsFnType};

use anyhow::{Context, Result};

pub fn generate_schema(prog: Module) -> Result<String> {
    let mut ctx = CodeGenCtx::new();
    let _ = ctx.parse(prog);
    todo!()
}

#[derive(Clone)]
enum FieldKind {
    Input,
    Object,
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
}

impl CodeGenCtx {
    fn new() -> Self {
        let schema = Schema::new();
        Self { schema }
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
            Stmt::Decl(Decl::TsTypeAlias(alias)) => match alias.id.sym.as_ref() {
                ident => {
                    let mut object_def = ObjectDef::new(ident.to_string());
                    self.parse_typed_fields(FieldKind::Object, &alias.type_ann)?
                        .into_iter()
                        .for_each(|f| object_def.field(f.object().unwrap()));

                    self.schema.object(object_def);
                    Ok(())
                }
            },
            _ => todo!(),
        }
    }

    /// Parse a GraphQL object definition. E.g.
    ///    type Person {
    ///        name: String!
    ///    }
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
    /// `field_name` is optional and is only used to generate names for Inputs.
    ///
    /// Important thing to note is that we only consider a sub-set of type, because
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
                        (
                            Type_::NamedType {
                                name: ident.sym.to_string(),
                            },
                            None,
                        )
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
                                // return type until we unwrap it from the Promise.
                                //
                                // Maybe we should move this match branch into its own dedicated function
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
                let mut args: Vec<InputValue> = Vec::with_capacity(params.len());
                for param in params {
                    args.push(self.parse_arg(field_name, param)?)
                }

                // Optional param can be anything, since we don't know if the return type is
                // optional until we parse it. `self.parse_type()` will make sure to return
                // the correct type if we are parsing return type
                let (ret_ty, _) = self.parse_type(field_name, &type_ann.type_ann, true)?;

                return Ok((ret_ty, Some(args)));
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

    fn parse_arg(&mut self, field_name: &str, param: &TsFnParam) -> Result<InputValue> {
        match param {
            TsFnParam::Ident(ident) => {
                let param_name = ident.id.sym.as_ref();
                let ty = &ident.type_ann.as_ref().unwrap().type_ann;
                let optional = ident.id.optional;

                let typ = match **ty {
                    // Anonymous type, parse the input and add
                    // to the schema
                    TsType::TsTypeLit(_) => {
                        // New name: field_name + ident (UpperCamelCase)
                        let name = format!(
                            "{}{}",
                            field_name
                                .chars()
                                .next()
                                .iter()
                                .map(|c| c.to_ascii_uppercase())
                                .chain(field_name.chars().skip(1))
                                .collect::<String>(),
                            param_name
                                .chars()
                                .next()
                                .into_iter()
                                .map(|c| c.to_ascii_uppercase())
                                .chain(param_name.chars().skip(1))
                                .collect::<String>()
                        );

                        let mut input_def = InputObjectDef::new(name.clone());

                        self.parse_typed_fields(FieldKind::Input, ty)?
                            .into_iter()
                            .for_each(|f| input_def.field(f.input().unwrap()));

                        self.schema.input(input_def);

                        if !optional {
                            Type_::NonNull {
                                ty: Box::new(Type_::NamedType { name }),
                            }
                        } else {
                            Type_::NamedType { name }
                        }
                    }
                    _ => {
                        let (ty, _) = self.parse_type(field_name, ty, ident.id.optional)?;
                        ty
                    }
                };
                Ok(InputValue::new(param_name.to_string(), typ))
            }
            // TODO: Error handling for invalid param
            _ => todo!(),
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
            // For now just assume type references are non-null
            TsType::TsTypeRef(_) => false,
            _ => todo!(),
        }
    }

    /// Return true if type is like: T | null or T | undefined
    fn is_nullable_union(ty: &TsType) -> bool {
        match ty {
            TsType::TsUnionOrIntersectionType(TsUnionOrIntersectionType::TsUnionType(
                TsUnionType { types, .. },
            )) => {
                for ty in types {
                    if Self::is_nullable(ty) {
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }

    /// Return the non-nullable type of a union. This will error if there are more than 2
    /// types, or there is no nullable type present
    /// Ex: "User | null"          -> User
    ///     "User | string | null" -> Error
    ///     "User | string"        -> Error
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

pub fn parse_sync(s: &str, opts: &str) -> Result<Program> {
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
        parse_sync(
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

    fn test(src: &str, expected: &str) {
        let prog = get_prog(src);

        let mut gen = CodeGenCtx::new();
        gen.parse(prog.module().unwrap()).unwrap();
        let out = gen.finish();
        println!("{}", out);
        assert_eq!(expected, out);
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
        );
    }

    #[test]
    fn it_parses_fields_with_args() {
        // Basic
        let src = "
        type User = { id: string; name: string; karma: number; }
        type Query = { findUser: (id: string, karma: number) => Promise<User>; }
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
              findUser(id: String!, karma: Int!): User!
            }
            "# },
        );

        // Multiple type literals
        let src = "
        type User = { id: string; name: string; karma: number; }
        type Query = { findUser: (input1: { id: string, karma: number }, input2: { name?: string }) => Promise<User>; }
        ";
        test(
            src,
            indoc! { r#"
            type User {
              id: String!
              name: String!
              karma: Int!
            }
            input FindUserInput1 {
              id: String!
              karma: Int!
            }
            input FindUserInput2 {
              name: String
            }
            type Query {
              findUser(input1: FindUserInput1!, input2: FindUserInput2!): User!
            }
            "# },
        );

        // Nullable return
        let src = "
        type User = { id: string; name: string; karma: number; }
        type Query = { findUser: (input: { id?: string; name?: string; }) => Promise<User | null>; }
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
              id: String
              name: String
            }
            type Query {
              findUser(input: FindUserInput!): User
            }
            "# },
        );

        // Required return
        let src = "
        type User = { id: string; name: string; karma: number; }
        type Query = { findUser: (input: { id?: string; name?: string; }) => Promise<User>; }
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
              id: String
              name: String
            }
            type Query {
              findUser(input: FindUserInput!): User!
            }
            "# },
        );
    }
}
