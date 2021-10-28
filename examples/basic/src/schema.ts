import { Input } from '../../../';
import { Person } from './types'

// export type User = {
//   id: string;
//   name: string;
//   karma: number | null;
// };

// type ___ExpandRecursively<T> = T extends object ? T extends infer O ? {
//   [K in keyof O]: ___ExpandRecursively<O[K]>;
// } : never : T;
// type ___Expand<T> = T extends infer O ? { [K in keyof O]: O[K] } : never;

export type User = Person;

// Here we use the `Input` marker type to tell TypeFirstQL
// that `GetUserInput` should generate a GraphQL Input.
//
// Notice how we can use Typescript's `Partial<T>` utility type
// to make every field of `User` optional!
export type GetUserInput = Input<Partial<User>>

export type Foo = User;

export type Query = {
  // This field will have two arguments
  getUser: (input: {
    user?: GetUserInput;
    karma?: number;
  }) => Promise<User[] | null>;
};

// export type Query = { findUser: (args: { user: Partial<User> }) => Promise<User | null> }

export type Mutation = {
  // Since the input is not a named and an exported type, TypeFirstQL will
  // generate an Input with the name `CreateUserInput` for you.
  createUser: (input: Omit<User, 'id'>) => Promise<{ name: string }>;

  updateUser: (
    input: { user: Partial<Omit<User, 'id'>> },
  ) => Promise<User | null>;
};