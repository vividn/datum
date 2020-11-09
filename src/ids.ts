const assembleId = function ({
  idField,
  id_delimiter = "__",
  partitionField,
  payload,
}: {
  idField: string | string[];
  id_delimiter?: string;
  partitionField?: string | string[];
  payload: { [key: string]: any };
}): string {
  const idFields = typeof idField === "string" ? [idField] : idField;

  const idComponents = idFields.reduce((result: string[], fieldName: string) => {
    const splitNonEscapedQuotes = fieldName
      .replace(/(?<!\\)'/g, "\xff\x00")
      .replace(/\\'/g, "'")
      .split("\xff\x00");

    // field names are even numbered indices, raw strings are odd
    const fullIdComponent = splitNonEscapedQuotes
      .reduce((combined: string[], value: string, index: number) => {
        if (index % 2 === 1) {
          combined.push(value);
        } else {
          if (value in payload) {
            combined.push(payload[value]);
          }
        }
        return combined;
      }, [])
      .join("");

    result.push(fullIdComponent);
    return result;
  }, []);

  return idComponents.join(id_delimiter);
};

module.exports = { assembleId };
