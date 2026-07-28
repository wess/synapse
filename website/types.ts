export type Kind = "home" | "docs" | "tutorial";

export type Toc = {
  label: string;
  id: string;
};

export type Page = {
  path: string;
  title: string;
  description: string;
  kind: Kind;
  body: string;
  toc?: Toc[];
  contract?: string;
};
