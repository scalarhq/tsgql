import { isTypeReferenceNode } from '@ts-morph/common/lib/typescript';
import {
  ExportedDeclarations,
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

enum GraphQLType {
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
  expanded: string[];

  constructor(project: Project, sourceFile: SourceFile) {
    this.project = project;
    this.sourceFile = sourceFile;
    this.checker = project.getTypeChecker();

    this.graphQlTypes = {};
    this.expanded = [];
  }

  generate(): string {
    this.collectTypeNames();
    this.generateReducedTypes();

    return this.expanded.join('\n');
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

    this.expandNode(node);

    const ty = node.getType().compilerType;
    this.expanded.push(
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

      for (const param of fn.getParameters()) {
        if (param.getName() === 'input') {
          this.expandParam(param);
        } else {
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

        // TODO: Allow "anonymous" return types, basically return types
        // defined in the resolver. We create a type for the user based on the resolver name:
        // getPerson: (id: string) => Promise<Person & { success: boolean}> -> GetPersonOutput
        //
        // This requires slight refactoring because we need to know all outputs/types ahead of time,
        // so we know whether to create a type or use an existing one the user defined.
        if (retNode.getType().isIntersection()) {
          fn.setReturnType(
            `Promise<___Expand<${this.typeToString(
              inner.getType().compilerType,
              inner.compilerNode
            )}>>`
          );
        } else {
          fn.setReturnType(
            `Promise<${this.typeToString(
              inner.getType().compilerType,
              inner.compilerNode,
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
    inAutoExpandableCtx = true
  ) {
    const ty = node.getType();
    if (ty.isIntersection()) {
      const { type } = node.getStructure();
      node.set({ type: `___Expand<${type}>` });
    }
    if (!inAutoExpandableCtx) {
      node.set({ type: this.typeToString(node.getType().compilerType) });
    }
  }

  expandParam(param: ParameterDeclaration) {
    const node = param.getTypeNode();

    if (node instanceof TypeReferenceNode) {
      switch (this.graphQlTypes[node.getTypeName().print()]) {
        case GraphQLType.Input: {
          this.expandNode(param, false);
          break;
        }
        default: {
          throw new Error('Field arguments can only be inputs');
        }
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
    }
    propSig.set({
      type: this.typeToString(
        propSig.getType().compilerType,
        propSig.compilerNode
      ),
    });
  }

  typeToString(
    ty: ts.Type,
    node?: ts.Node,
    flags: ts.TypeFormatFlags = ts.TypeFormatFlags.NoTruncation |
      ts.TypeFormatFlags.InTypeAlias
  ) {
    const str = this.checker.compilerObject.typeToString(ty, node, flags);

    // For some reason type checker mysteriously drops unions containing
    // `| null` and `| undefined`, and perplexingly enough, node.isUnion() returns false
    // even if node.kind === 185 (UnionType)...
    if (node?.kind === ts.SyntaxKind.UnionType) {
      const text = node?.getText();
      if (text?.includes('| null')) {
        return str + ' | null';
      }
      if (text?.includes('| undefined')) {
        return str + ' | undefined';
      }
    }

    return str;
  }
}
