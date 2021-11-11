import {
  Node,
  FunctionTypeNode,
  ParameterDeclaration,
  Project,
  PropertySignature,
  SourceFile,
  ts,
  Type,
  TypeAliasDeclaration,
  TypeChecker,
  TypeLiteralNode,
  TypeReferenceNode,
} from 'ts-morph';

const DefaultFormatFlags =
  ts.TypeFormatFlags.NoTruncation | ts.TypeFormatFlags.InTypeAlias;

export enum GraphQLType {
  Type,
  Input,
}

const parentless = <T extends { parent: any }>(node: T): Omit<T, 'parent'> => {
  const { parent: _, ...rest } = node;
  return rest;
};

export class TypeReducer {
  project: Project;
  sourceFile: SourceFile;
  checker: TypeChecker;

  graphQlTypes: Record<string, GraphQLType>;
  acknowledgedTypes: Record<string, boolean>;

  expanded: Record<string, string>;
  finalExpansions: string[];

  constructor(project: Project, sourceFile: SourceFile) {
    this.project = project;
    this.sourceFile = sourceFile;
    this.checker = project.getTypeChecker();

    this.graphQlTypes = {};
    this.acknowledgedTypes = {};

    this.expanded = {};
    this.finalExpansions = [];
  }

  generate(): [output: string, manifest: Record<string, GraphQLType>] {
    this.collectTypeNames();
    this.generateReducedTypes();

    return [this.finalExpansions.join('\n'), this.graphQlTypes];
  }

  addTestPreludeTypes() {
    this.project.createSourceFile(
      './prelude.ts',
      'export type Input<T extends Record<string, any>> = T;'
    );
  }

  addAdditionalFile(name: string, code: string) {
    this.project.createSourceFile(name, code)
  }

  // First pass, collect type names
  collectTypeNames() {
    const exported = this.sourceFile.getExportedDeclarations();
    for (const [name, [decl]] of exported) {
      switch (decl.getKind()) {
        case ts.SyntaxKind.TypeAliasDeclaration: {
          const { type } = (decl as TypeAliasDeclaration).getStructure();
          if ((type as string).indexOf('Input<') === 0) {
            this.graphQlTypes[name] = GraphQLType.Input;
          } else {
            this.graphQlTypes[name] = GraphQLType.Type;
          }
          break;
        }
        default: {
          break;
        }
      }
    }

    const imported = this.sourceFile.getImportDeclarations();
    for (const decl of imported) {
      for (const imp of decl.getNamedImports()) {
        this.acknowledgedTypes[imp.getName()] = true;
      }
    }
  }

  generateReducedTypes() {
    const exported = this.sourceFile.getExportedDeclarations();
    for (const [name, [decl]] of exported) {
      switch (decl.getKind()) {
        case ts.SyntaxKind.TypeAliasDeclaration: {
          this.visitTypeAliasNode(decl as TypeAliasDeclaration);
          break;
        }
        default: {
          break;
        }
      }
    }
  }

  visitTypeAliasNode(node: TypeAliasDeclaration) {
    const name = node.getName();
    if (name === 'Query' || name === 'Mutation') {
      this.visitQueryOrMutationDecl(node);
    }

    switch (node.getTypeNode()?.getKind()) {
      // Resolving type references
      case ts.SyntaxKind.TypeReference: {
        const name = node.getTypeNode()?.getText() || '';
        // Don't do anything if it's a graphql type
        if (this.graphQlTypes[name] !== undefined) {
          break;
        }
        if (this.acknowledgedTypes[name]) {
          this.expandNode(node, false, true);
        }
        // If we haven't visited this type, it probably means it's a utility type like
        // Partial<T>, Omit<T>, etc. and we want to expand it
        break;
      }
    }

    this.expandNode(node);

    const ty = node.getType().compilerType;
    this.finalExpansions.push(
      `type ${name} = ${this.checker.compilerObject.typeToString(
        ty,
        // For some reason setting this param to undefined will make
        // this fn omit `undefined` in union, e.g. string | undefined -> string
        node.compilerNode,
        ts.TypeFormatFlags.NoTruncation | ts.TypeFormatFlags.InTypeAlias
      )}`
    );
  }

  visitQueryOrMutationDecl(node: TypeAliasDeclaration) {
    const tyNode = node.getTypeNode() as TypeLiteralNode;
    for (const member of tyNode.getMembers()) {
      if (member.getKind() !== ts.SyntaxKind.PropertySignature) {
        throw new Error('Type `Query` or `Mutation` can only have members');
      }

      const propSig = member as PropertySignature;
      const tyNode = propSig.getTypeNode();

      if (tyNode?.getKind() !== ts.SyntaxKind.FunctionType) {
        throw new Error('Type `Query` or `Mutation` can only have members');
      }
      const fn = tyNode as FunctionTypeNode;

      const params = fn.getParameters();
      if (params.length === 1) {
        this.expandParam(params[0]);
      } else {
        for (const param of params) {
          this.expandNode(param, false);
        }
      }

      const ret = fn.getReturnTypeNode();
      if (ret) {
        if (ret.getKind() !== ts.SyntaxKind.TypeReference) {
          throw new Error(
            '`Query` or `Mutation` resolvers must return a Promise'
          );
        }

        const retNode = ret as TypeReferenceNode;
        if (retNode.getTypeName().getText() !== 'Promise') {
          throw new Error(
            '`Query` or `Mutation` resolvers must return a Promise'
          );
        }

        const [inner] = retNode.getTypeArguments();

        if (retNode.getType().isIntersection()) {
          fn.setReturnType(
            `Promise<___Expand<${this.typeToString(inner.getType(), inner)}>>`
          );
        } else {
          fn.setReturnType(
            `Promise<${this.typeToString(
              inner.getType(),
              inner,
              ts.TypeFormatFlags.NoTruncation
            )}>`
          );
        }
      }
    }
  }

  expandNode(
    node: {
      getType(): Type<ts.Type>;
      getStructure(): any;
      set(obj: Record<string, any>): any;
    },
    inAutoExpandableCtx = true,
    forceExpansion = false,
    enclosingNode?: Node
  ) {
    const ty = node.getType();
    if (ty.isIntersection() || forceExpansion) {
      const { type } = node.getStructure();
      node.set({ type: `___Expand<${type}>` });
    }
    if (!inAutoExpandableCtx) {
      node.set({
        type: this.typeToString(node.getType(), enclosingNode || (node as any)),
      });
    }
  }

  expandParam(param: ParameterDeclaration) {
    const node = param.getTypeNode();

    if (node instanceof TypeReferenceNode) {
      const graphqlTy = this.graphQlTypes[node.getTypeName().print()];
      if (graphqlTy) {
        switch (graphqlTy) {
          case GraphQLType.Input: {
            this.expandNode(param, false);
            break;
          }
          default: {
            throw new Error('Field arguments can only be inputs');
          }
        }
      } else {
        // We have a type constructed from type utilities: e.g. Partial<User>
        this.expandNode(param, false);
      }
    } else if (node instanceof TypeLiteralNode) {
      for (const prop of node.getProperties()) {
        this.expandProperty(prop);
      }
    }
  }

  expandProperty(propSig: PropertySignature) {
    const tyNode = propSig.getTypeNode();
    if (tyNode instanceof TypeReferenceNode) {
      const graphQlTy = this.graphQlTypes[tyNode.getTypeName().print()];
      if (graphQlTy !== undefined) {
        switch (graphQlTy) {
          case GraphQLType.Input: {
            return;
          }
          default: {
            throw new Error('Field arguments can only be inputs');
          }
        }
      }

      this.expandNode(propSig, false, true);
      return;
    }

    propSig.set({
      type: this.typeToString(propSig.getType(), propSig),
    });
  }

  typeToString(
    ty: Type<ts.Type>,
    node?: Node,
    flags: ts.TypeFormatFlags = ts.TypeFormatFlags.NoTruncation |
      ts.TypeFormatFlags.InTypeAlias
  ): string {
    switch (node?.getKind()) {
      case ts.SyntaxKind.TypeReference: {
        const name = (node as TypeReferenceNode).getTypeName().getText();
        if (this.graphQlTypes[name] !== undefined) {
          return name;
        }
        return this.checker.compilerObject.typeToString(
          ty.compilerType,
          node?.compilerNode,
          flags
        );
      }
      default: {
        const f = this.checker.compilerObject.typeToString(
          ty.compilerType,
          node?.compilerNode,
          flags
        );
        return f;
      }
    }
  }
}
