import {
  DatumData,
  DatumMetadata,
  isOccurredData,
} from "../documentControl/DatumDocument";

export function defaultIdComponents({
  data,
  meta,
}: {
  data: DatumData;
  meta?: DatumMetadata;
}): {
  defaultIdParts: string[];
  defaultPartitionParts?: string[];
} {
  const defaultIdParts = isOccurredData(data)
    ? ["%occurTime%"]
    : meta?.createTime
      ? ["%?createTime%c"] // notice the trailing "c" here
      : Object.keys(data).map(
          (key) => `%${key.replace(/%/g, String.raw`\%`)}%`,
        );
  const defaultPartitionParts = "field" in data ? ["%field%"] : undefined;
  return { defaultIdParts, defaultPartitionParts };
}
