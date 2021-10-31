import { Input } from '../../../';

export type User = {
  id: string;
  name: string;
  karma: number | null;
};


// Here we use the `Input` marker type to tell tsgql
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

export type Mutation = {
  // Since the input is not a named and an exported type, tsgql will
  // generate an Input with the name `CreateUserInput` for you.
  createUser: (input: Omit<User, 'id'>) => Promise<{ name: string }>;

  updateUser: (
    input: { user?: Partial<User> },
  ) => Promise<User | null>;
};
