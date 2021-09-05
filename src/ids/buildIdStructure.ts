import { defaults } from "../input/defaults";

export type buildIdStructureType = {
  idParts: string | string[];
  partition?: string | string[];
  delimiter?: string;
};
export const buildIdStructure = function ({
  idParts,
  partition,
  delimiter = defaults.idDelimiter,
}: buildIdStructureType): string {
  // % is reserved for field names, escape it
  delimiter = delimiter === "%" ? "\\%" : delimiter;

  const partitionAndId =
    partition === undefined ? [idParts] : [partition, idParts];
  const structurizedPartitionId = partitionAndId.map((idOrPartition) => {
    const inputArray =
      typeof idOrPartition === "string" ? [idOrPartition] : idOrPartition;

    // for convenience, user can use just "%fieldName" rather than the full "%fieldName%", this adds the missing percent at the end
    const appendedTrailingPercent = inputArray.map((idComponent) => {
      // skip escaped percents
      const percentCount = (idComponent.match(/(?<!\\)%/g) || []).length;
      return percentCount % 2 === 0 ? idComponent : idComponent + "%";
    });

    return appendedTrailingPercent.join(delimiter);
  });

  return structurizedPartitionId.join(":");
};
