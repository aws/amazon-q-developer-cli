export type PrefDefault = boolean | string | string[] | number | null | never[]

export type Pref = {
  id: string,
  title: string,
  description?: string,
  example?: string,
  type: string,
  inverted?: boolean
  default: PrefDefault
  popular?: boolean
  options?: string[]
}

export type RichText = {
  content: string,
  tag: string
}

export type InstallCheck = {
  id: string
  installKey?: "dotfiles" | "accessibility" | "inputMethod"
  title: string
  description: string[]
  image?: string
  action: string
  explainer?: {
    title: string,
    steps: RichText[][]
  }
};