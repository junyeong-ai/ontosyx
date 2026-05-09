export type ClientPage<T extends { next_cursor?: string | null }> = Omit<
  T,
  "next_cursor"
> & {
  next_cursor?: string;
};
