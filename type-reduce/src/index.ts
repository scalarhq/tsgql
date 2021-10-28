import { Project, SourceFile } from 'ts-morph';
import { TypeReducer } from './lib';

const injectedTypes = `
type ___ExpandRecursively<T> = T extends object ? T extends infer O ? {
    [K in keyof O]: ___ExpandRecursively<O[K]>;
} : never : T;
type ___Expand<T> = T extends infer O ? { [K in keyof O]: O[K] } : never;
`;

const testPrelude = 'export type Input<T extends Record<string, any>> = T;';

type Opts = {
  tsconfigPath?: string;
  path?: string;
  code?: string;
  additional?: { name: string; src: string }[];
  test?: boolean
};

export const createReducer = ({
  tsconfigPath,
  path,
  code,
  additional,
  test,
}: Opts) => {
  const project = new Project(test ? undefined : { tsConfigFilePath: tsconfigPath });

  additional?.forEach(({ name, src: code }) =>
    project.createSourceFile(name, code)
  );

  if (test) {
    project.createSourceFile('./prelude.ts', testPrelude);
  }

  // With strict on Typescript turns optional types into unions:
  // optionalType?: string -> optionalType?: string | undefined
  project.compilerOptions.set({ strict: false, strictNullChecks: true });

  let sourceFile: SourceFile | undefined;
  if (path) {
    sourceFile = project.getSourceFile(path);
    if (!sourceFile) {
      throw new Error('Schema file not found');
    }
  } else if (code) {
    sourceFile = project.createSourceFile('index.ts', code);
  }

  if (!sourceFile) throw new Error('No schema input file');

  sourceFile.insertText(0, injectedTypes);

  const diagnostics = project.getPreEmitDiagnostics();
  if (diagnostics.length) {
    diagnostics.forEach(d => console.error(d.compilerObject.messageText))
    throw new Error('Aborting because of TSC errors');
  }

  return new TypeReducer(project, sourceFile);
};

export * from './types';
