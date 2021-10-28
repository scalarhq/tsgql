import { join } from 'path';
import { Project } from 'ts-morph';
import { GraphQLType, TypeReducer } from '../lib';
import { createReducer as createReducer_ } from '../index';

describe('TypeReducer', () => {
  describe('First pass', () => {
    it('Collects exported declarations', () => {
      const reducer = createReducer(`
			import { Input } from './prelude'

			export type User = { firstName: string; age: number }
			export type FindUserInput = Input<{ firstName: string; }>
			export type Query = { findUser: (args: { input: FindUserInput }) => Promise<User | null> }
			`);

      reducer.collectTypeNames();

      expect(reducer.graphQlTypes).toEqual({
        User: GraphQLType.Type,
        FindUserInput: GraphQLType.Input,
        Query: GraphQLType.Type,
      });
    });

    it('Acknowledges non-exported types', () => {
      const reducer = createReducer(
        `
			import { Input } from './prelude'
			import { Person } from './person'

			export type User = Person
			export type FindUserInput = Input<{ firstName: string; }>
			export type Query = { findUser: (args: { input: FindUserInput }) => Promise<User | null> }
			`,
        {
          name: './person.ts',
          src: 'export type Person = { firstName: string, age: number }',
        }
      );

      reducer.collectTypeNames();

      expect(reducer.graphQlTypes).toEqual({
        User: GraphQLType.Type,
        FindUserInput: GraphQLType.Input,
        Query: GraphQLType.Type,
      });

      expect(reducer.acknowledgedTypes).toEqual({
        Person: true,
        Input: true,
      });
    });
  });

  describe('Type reduction', () => {
    // TODO: Errors on non-nullable unions
    // For some reason TS's type checker will do this, so
    // we manually add it back
    it("Doesn't drop nullable unions", () => {
      const reducer = createReducer(`
        export type User = { name: string | null }
        export type Query = { findUser: (args: { name: string }) => Promise<User | null> }
        `);

      const [output] = reduceTypes(reducer);
      expect(output).toEqual(`type User = { name: string | null; }
type Query = { findUser: (args: { name: string; }) => Promise<User | null>; }
`);
    });

    describe('Utility type expansion', () => {
      it('Expands utility types in declarations', () => {
        const reducer = createReducer(`
        export type User = { name: string | null, id: number, age: number }
        export type Person = Omit<User, 'age'>
        `);

        const [output] = reduceTypes(reducer);
        expect(output)
          .toEqual(`type User = { name: string | null; id: number; age: number; }
type Person = { name: string | null; id: number; }
`);
      });

      it('Expands utility types as input', () => {
        const reducer = createReducer(`
        export type User = { name: string | null, id: number, age: number }
        export type Query = { findUser: (args: Partial<User>) => Promise<User | null> }
        `);

        const [output] = reduceTypes(reducer);
        expect(output)
          .toEqual(`type User = { name: string | null; id: number; age: number; }
type Query = { findUser: (args: { name?: string | null | undefined; id?: number | undefined; age?: number | undefined; }) => Promise<User | null>; }
`);
      });

      it('Expands utility types in input arg', () => {
        const reducer = createReducer(`
              export type User = { name: string | null, id: number, age: number }
              export type Query = { findUser: (args: { user: Partial<User> }) => Promise<User | null> }
              `);

        const [output] = reduceTypes(reducer);
        expect(output)
          .toEqual(`type User = { name: string | null; id: number; age: number; }
type Query = { findUser: (args: { user: { name?: string | null | undefined; id?: number | undefined; age?: number | undefined; }; }) => Promise<User | null>; }
`);
      });
    });
  });
});

function createReducer(
  source: string,
  ...other: { name: string; src: string }[]
) {
  const reducer = createReducer_({
    code: source,
    additional: other,
    test: true,
  });

  return reducer;
}

function reduceTypes(
  reducer: TypeReducer
): ReturnType<typeof reducer['generate']> {
  const [output, manifest] = reducer.generate();
  const project = new Project();
  const sourceFile = project.createSourceFile('./index.ts', output);
  sourceFile.formatText();

  return [sourceFile.getFullText(), manifest];
}
