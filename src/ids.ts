const assembleId = function ({
  idField = "%meta.occurTime%",
  delimiter = "__",
  partitionField = "%field%",
  payload,
}: {
  idField?: string | string[];
  delimiter?: string;
  partitionField?: string | string[];
  payload: { [key: string]: any };
}): {id: string, structure: string} {
  const partitionStructure = buildIdStructure(partitionField, delimiter)
  const idStructure = buildIdStructure(idField, delimiter)

  const partition = idFromStructure(partitionStructure, payload)
  const subId = idFromStructure(idStructure, payload)
  if (partition.length > 0) {
    const structure = `${partitionStructure}:${idStructure}`
    const id = `${partition}:${subId}`
    return {id, structure}
  } else {
    return {id: subId, structure: idStructure}
  }
};

const buildIdStructure = function (
  idOrPartition: string | string[],
  delimiter: string
): string {
  delimiter = delimiter === '%' ? '\\%' : delimiter
  const inputArray = typeof idOrPartition === "string" ? [idOrPartition] : idOrPartition;
  
  // for convienience, user can use just "%fieldName" rather than the full "%fieldName%", this adds the missing percent at the end
  const appendedTrailingPercent = inputArray.map((idComponent) => {
    // skip escaped percents
    const percentCount = (idComponent.match(/(?<!\\)%/g) || []).length
    return percentCount % 2 === 0 ? idComponent : idComponent + "%"
  })

  return appendedTrailingPercent.join(delimiter)
}

const idFromStructure = function(
  structure: string,
  payload: { [key: string]: any }
): string {
  // split apart and also replace the escaped %s with normal percents
  const splitOutRawStrings = structure
    .replace(/(?<!\\)%/g, "\xff\x00")
    .replace(/\\%/g, "%")
    .split("\xff\x00");

  // raw strings are even numbered indices, field names are odd
  const interpolatedFields = splitOutRawStrings.reduce((combined: string[], strPart: string, index: number) => {
    if (index % 2 === 0) {
      combined.push(strPart);
    } else {
      // retrieve deeper values with dot notation
      const extractedValue = strPart
        .split(".")
        .reduce((o, i) => (o === undefined ? undefined : o[i]), payload);
      if (extractedValue !== undefined) {
        const valueAsString =
          typeof extractedValue === "string"
            ? extractedValue
            : JSON.stringify(extractedValue);
        combined.push(valueAsString);
      }
    }
    return combined;
  }, [])
  .join("");

  return interpolatedFields
}

module.exports = { assembleId };
