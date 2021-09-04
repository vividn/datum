import { DatumData, isOccurredData } from "../documentControl/DatumDocument";

export function defaultIdComponents({ data }: { data: DatumData }): {
  defaultIdParts: string[];
  defaultPartitionParts?: string[];
} {
  const defaultIdParts = isOccurredData(data)
    ? ["%occurTime%"]
    : Object.keys(data).map((key) => `%${key.replace(/%/g, String.raw`\%`)}%`);
  const defaultPartitionParts = "field" in data ? ["%field%"] : undefined;
  return { defaultIdParts, defaultPartitionParts };
}