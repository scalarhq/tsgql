import { Input } from './types';

type ___ExpandRecursively<T> = T extends object ? T extends infer O ? {
  [K in keyof O]: ___ExpandRecursively<O[K]>;
} : never : T;
type ___Expand<T> = T extends infer O ? { [K in keyof O]: O[K] } : never;

export type User = {
  id: string;
  name: string;
  karma: number;
};

export type Loot = {
  rarity: number;
  name: string;
};

export type Player = {
  user: User;
  level: number;
};

export type GetUserInput = Input<Partial<User>>

export type Query = {
  // findUser: (
  //   input: Partial<Pick<User, 'id' | 'name'>>,
  //   input2: User
  // ) => Promise<User | null>;
  getUser: (input: {
    // user?: Partial<Pick<User, 'id' | 'name'>>;
    user?: GetUserInput;
    karma?: number;
  }) => Promise<User[] | null>;
};

export type Mutation = {
  createUser: (input: { user: Partial<Omit<User, 'id'>> }) => Promise<User | null>;
  createLoot: (
    input: Partial<Omit<Loot, 'rarity'>>
  ) => Promise<CreateLootOutput>;
};

export type CreateLootOutput = {
  success: boolean;
  loot?: Loot;
};
