import * as fs from 'fs';
import * as path from 'path';
import {
  Node,
  FunctionTypeNode,
  NamedNodeSpecific,
  NamedNodeSpecificBase,
  ParameterDeclaration,
  Project,
  PropertySignature,
  ts,
  Type,
  TypeAliasDeclaration,
  TypeAliasDeclarationStructure,
  TypedNode,
  TypeLiteralNode,
  ReturnTypedNode,
  TypeReferenceNode,
} from 'ts-morph';

const parentless = <T extends { parent: any }>(node: T): Omit<T, 'parent'> => {
  const { parent: _, ...rest } = node;
  return rest;
};

const project = new Project();
const checker = project.getTypeChecker();

const prelude = `
type ___ExpandRecursively<T> = T extends object ? T extends infer O ? {
    [K in keyof O]: ___expandRecursively<O[K]>;
} : never : T;
type ___Expand<T> = T extends infer O ? { [K in keyof O]: O[K] } : never;
`;

const fileText = fs.readFileSync(path.join(__dirname, './src/input.ts'), {
  encoding: 'utf8',
});

const sourceFile = project.createSourceFile('main.ts', prelude + fileText);

const typeAliases = sourceFile.getTypeAliases();

const expanded: string[] = [];

const exported = sourceFile.getExportedDeclarations();

for (const [name, [decl]] of exported) {
  switch (decl.getKind()) {
    case ts.SyntaxKind.TypeAliasDeclaration: {
      visitTypeAliasNode(decl as TypeAliasDeclaration);
      break;
    }
    default: {
      break;
    }
  }
}

function visitTypeAliasNode(node: TypeAliasDeclaration) {
  const name = node.getName();
  if (name === 'Query' || name === 'Mutation') {
    visitQueryOrMutationDecl(node);
  }

  expandNode(node);

  const ty = node.getType().compilerType;
  expanded.push(
    `type ${name} = ${checker.compilerObject.typeToString(
      ty,
      // For some reason setting this param to undefined will make
      // this fn omit `undefined` in union, e.g. string | undefined -> string
      node.compilerNode,
      ts.TypeFormatFlags.NoTruncation | ts.TypeFormatFlags.InTypeAlias
    )}`
  );
}

function visitQueryOrMutationDecl(node: TypeAliasDeclaration) {
  const tyNode = node.getTypeNode() as TypeLiteralNode;
  for (const member of tyNode.getMembers()) {
    if (member.getKind() !== ts.SyntaxKind.PropertySignature) {
      throw new Error('Type `Query` or `Mutation` can only have function members');
    }

    const propSig = member as PropertySignature;
    const tyNode = propSig.getTypeNode();

    if (tyNode?.getKind() !== ts.SyntaxKind.FunctionType) {
      throw new Error('Type `Query` or `Mutation` can only have function members');
    }
    const fn = tyNode as FunctionTypeNode;

    for (const param of fn.getParameters()) {
      expandNode(param, false);
    }

    const ret = fn.getReturnTypeNode();
    if (ret) {
      if (ret.getKind() !== ts.SyntaxKind.TypeReference) {
        throw new Error('`Query` or `Mutation` resolvers must return a Promise');
      }

      const retNode = ret as TypeReferenceNode;
      if (retNode.getTypeName().getText() !== 'Promise') {
        throw new Error('`Query` or `Mutation` resolvers must return a Promise');
      }

      const [inner] = retNode.getTypeArguments();

      // TODO: Allow "anonymous" return types, basically return types
      // defined in the resolver. We create a type for the user based on the resolver name:
      // getPerson: (id: string) => Promise<Person & { success: boolean}> -> GetPersonOutput
      //
      // This requires slight refactoring because we need to know all outputs/types ahead of time,
      // so we know whether to create a type or use an existing one the user defined.
      if (retNode.getType().isIntersection()) {
        console.log('ExPANDING')
        fn.setReturnType(
          `Promise<___Expand<${typeToString(
            inner.getType().compilerType,
            inner.compilerNode
          )}>>`
        );
      } else {
        console.log('Not supposed to expand')
        fn.setReturnType(
          `Promise<${typeToString(
            inner.getType().compilerType,
            inner.compilerNode,
            ts.TypeFormatFlags.NoTruncation
          )}>`
        );
      }
    }
  }
}

function expandNode(
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
    node.set({ type: typeToString(node.getType().compilerType) });
  }
}

function typeToString(ty: ts.Type, node?: ts.Node, flags: ts.TypeFormatFlags = ts.TypeFormatFlags.NoTruncation | ts.TypeFormatFlags.InTypeAlias) {
  const str = checker.compilerObject.typeToString(
    ty,
    node,
    flags
  );

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

console.log(expanded);
const output = expanded.join('\n')
fs.writeFileSync('./cleaned.ts', output)
