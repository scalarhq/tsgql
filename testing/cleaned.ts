type User = { id: string; name: string; karma: number; }
type Loot = { rarity: number; name: string; }
type Player = { user: User; level: number; }
type GetUserInput = { id?: string; name?: string; karma?: number; }
type Query = { getUser: (args: {    user?: GetUserInput;    karma?: number;}) => Promise<User[] | null>; }
type CreateLootOutput = { success: boolean; loot?: Loot; }