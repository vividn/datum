// TODO: Remove this file after proper specs are setup

type FieldSpec = {
  kind?: "occur" | "start" | "switch";
  color?: string;
  y?: number; // 0 to 1
  height?: number; // 0 to 1
  zIndex?: number; // defaults to y
  states?: Record<string, Omit<FieldSpec, "states" | "kind">>;
  stateY?: number; // 0 to 1, relative to whole field
  stateHeight?: number; // 0 to 1, relative to whole field
};

export const FIELD_SPECS: Record<string, FieldSpec> = {} as const;
