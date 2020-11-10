const assembleId = function ({
  idField = "meta.occurTime",
  idDelimiter = "__",
  partitionField = "field",
  payload,
}: {
  idField?: string | string[];
  idDelimiter?: string;
  partitionField?: string | string[];
  payload: { [key: string]: any };
}): string {
  const idSection = buildDelimitedFromFields({
    fields: idField,
    delimiter: idDelimiter,
    payload,
  });
  const partitionSection = buildDelimitedFromFields({
    fields: partitionField,
    delimiter: idDelimiter,
    payload,
  });

  const fullId =
    partitionSection.length > 0
      ? `${partitionSection}:${idSection}`
      : idSection;
  return fullId;
};

const buildDelimitedFromFields = function ({
  fields,
  delimiter,
  payload,
}: {
  fields: string | string[];
  delimiter: string;
  payload: { [key: string]: any };
}) {
  const arrFields = typeof fields === "string" ? [fields] : fields;
  const allComponents = arrFields.reduce(
    (result: string[], fieldName: string) => {
      const splitNonEscapedQuotes = fieldName
        .replace(/(?<!\\)'/g, "\xff\x00")
        .replace(/\\'/g, "'")
        .split("\xff\x00");

      // field names are even numbered indices, raw strings are odd
      const joinedRawStringsAndFields = splitNonEscapedQuotes
        .reduce((combined: string[], strPart: string, index: number) => {
          if (index % 2 === 1) {
            combined.push(strPart);
          } else {
            // retrieve deeper values with dot notation
            const extractedValue = strPart
              .split(".")
              .reduce((o, i) => o[i], payload);
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

      result.push(joinedRawStringsAndFields);
      return result;
    },
    []
  );

  return allComponents.join(delimiter);
};

module.exports = { assembleId };
