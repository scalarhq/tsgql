use indoc::indoc;
use std::{collections::HashMap, mem, sync::Arc};

use apollo_encoder::{Field, InputValue, ObjectDef, Schema, Type_};
use swc::{
    config::{JsMinifyOptions, JscTarget, Options, ParseOptions, SourceMapsConfig},
    try_with_handler, Compiler,
};
use swc_common::{FileName, FilePathMapping, SourceMap};
use swc_ecmascript::ast::{
    BindingIdent, Decl, Module, ModuleItem, Stmt, TsArrayType, TsEntityName, TsFnParam,
    TsKeywordType, TsKeywordTypeKind, TsPropertySignature, TsType, TsTypeAliasDecl, TsTypeAnn,
    TsTypeRef, TsUnionOrIntersectionType, TsUnionType,
};
use swc_ecmascript::ast::{Program, TsFnOrConstructorType, TsFnType};

use anyhow::{Context, Result};

pub fn generate_schema(prog: Module) -> Result<String> {
    let mut ctx = CodeGenCtx::new();
    ctx.parse(prog);
    todo!()
}

enum RootOperation {
    Query,
    Mutation,
}

enum ParsedType {
    Basic(Type_),
    WithArgs(Type_, Vec<InputValue>),
    Identifier(Type_),
    List(Type_),
    ObjLiteral(HashMap<String, Type_>),
}

struct CodeGenCtx {
    schema: Schema,
    inputs: HashMap<String, InputValue>,
}

impl CodeGenCtx {
    fn new() -> Self {
        let schema = Schema::new();
        Self {
            schema,
            inputs: HashMap::new(),
        }
    }

    fn schema(&self) -> &Schema {
        &self.schema
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
                    let object_def = self.parse_object_def(alias)?;
                    self.schema.object(object_def);
                    Ok(())
                }
            },
            _ => todo!(),
        }
    }

    fn parse_input_def(&mut self) {
        todo!()
    }

    fn parse_output_def(&mut self) {
        todo!()
    }

    // fn parse_root_op(&mut self, alias: TsTypeAliasDecl, kind: RootOperation) -> {

    // }

    /// Parse a GraphQL object definition. E.g.
    ///    type Person {
    ///        name: String!
    ///    }
    fn parse_object_def(&mut self, alias: TsTypeAliasDecl) -> Result<ObjectDef> {
        let mut object_def = ObjectDef::new(alias.id.sym.to_string());
        match *alias.type_ann {
            TsType::TsTypeLit(lit) => {
                for member in lit.members {
                    let prop_sig = member.ts_property_signature().unwrap();
                    let field = self.parse_field(prop_sig)?;
                    object_def.field(field);
                }
            }
            _ => todo!(),
        };

        Ok(object_def)
    }

    fn parse_field(&mut self, prop_sig: TsPropertySignature) -> Result<Field> {
        let key = prop_sig.key.ident().unwrap().sym.to_string();
        match self.parse_type(&prop_sig.type_ann.unwrap().type_ann, prop_sig.optional)? {
            (ty, None) => Ok(Field::new(key, ty)),
            (ty, Some(args)) => {
                let mut field = Field::new(key, ty);
                for arg in args {
                    field.arg(arg)
                }
                Ok(field)
            }
        }
    }

    fn parse_type(
        &mut self,
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
                    ty: Box::new(self.parse_type(elem_type, true)?.0),
                },
                None,
            ),
            TsType::TsTypeRef(TsTypeRef { type_name, .. }) => {
                if let TsEntityName::Ident(ident) = type_name {
                    (
                        Type_::NamedType {
                            name: ident.sym.to_string(),
                        },
                        None,
                    )
                } else {
                    todo!()
                }
            }
            TsType::TsFnOrConstructorType(TsFnOrConstructorType::TsFnType(TsFnType {
                params,
                type_ann,
                ..
            })) => {
                let mut args: Vec<InputValue> = Vec::with_capacity(params.len());
                for param in params {
                    args.push(self.parse_arg(param)?)
                }

                let optional = Self::is_nullable_union(&type_ann.type_ann);

                let (ret_ty, _) = self.parse_type(&type_ann.type_ann, optional)?;

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

    fn parse_arg(&mut self, param: &TsFnParam) -> Result<InputValue> {
        match param {
            TsFnParam::Ident(ident) => {
                let name = ident.id.sym.as_ref();
                let (ty, _) = self.parse_type(
                    &ident.type_ann.as_ref().unwrap().type_ann,
                    ident.id.optional,
                )?;
                Ok(InputValue::new(name.to_string(), ty))
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
    /// Return true if type is like: T | null or T | undefined
    fn is_nullable_union(ty: &TsType) -> bool {
        match ty {
            TsType::TsUnionOrIntersectionType(TsUnionOrIntersectionType::TsUnionType(
                TsUnionType { types, .. },
            )) => {
                for ty in types {
                    if let TsType::TsKeywordType(TsKeywordType { kind, .. }) = **ty {
                        match kind {
                            TsKeywordTypeKind::TsNullKeyword
                            | TsKeywordTypeKind::TsUndefinedKeyword => {
                                return true;
                            }
                            _ => {}
                        };
                    }
                }
                false
            }
            _ => false,
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
                &handler,
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
    fn object_def() {
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
    fn object_def_arrays() {
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
}
