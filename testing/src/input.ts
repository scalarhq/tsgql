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

export type CreatePlayerInput = Partial<Player>;

export type CreatePlayerAndUserInput = Partial<Omit<Player, 'user'>> &
  Partial<User>;

export type Query = {
  findUser: (input: Partial<Pick<User, 'id' | 'name'>>) => Promise<User | null>;
};

export type Mutation = {
	createUser: (input: Partial<Omit<User, 'id'>>) => Promise<User | null>
  createLoot: (input: Partial<Omit<Loot, 'rarity'>>) => Promise<CreateLootOutput>
}

export type CreateLootOutput = {
  success: boolean,
  loot?: Loot
}