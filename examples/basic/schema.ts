import { Input } from '../../';

export type User = {
  id: string;
  name: string;
  karma: number;
};

// Here we use the `Input` marker type to tell TypeFirstQL
// that `GetUserInput` should generate a GraphQL Input.
//
// Notice how we can use Typescript's `Partial<T>` utility type
// to make every field of `User` optional!
export type GetUserInput = Input<Partial<User>>

export type Query = {
  // This field will have two arguments
  getUser: (input: {
    user?: GetUserInput;
    karma?: number;
  }) => Promise<User[] | null>;
};

export type Mutation = {
  // Since the input is not a named and an exported type, TypeFirstQL will
  // generate an Input with the name `CreateUserInput` for you.
  createUser: (input: Partial<Omit<User, 'id'>>) => Promise<User | null>;


  updateUser: (
    input: { user: Partial<Pick<User, 'id' | 'name'>> },
  ) => Promise<User | null>;
};

