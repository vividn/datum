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

  // Handle the id parts only (field is handled separately in assembleId)
  const inputArray =
    typeof idParts === "string" ? [idParts] : idParts;

  // for convenience, user can use just "%fieldName" rather than the full "%fieldName%", this adds the missing percent at the end
  const appendedTrailingPercent = inputArray.map((idComponent) => {
    // skip escaped percents
    const percentCount = (idComponent.match(/(?<!\\)%/g) || []).length;
    return percentCount % 2 === 0 ? idComponent : idComponent + "%";
  });

  return appendedTrailingPercent.join(delimiter);
};
