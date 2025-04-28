import { addIdAndMetadata } from "../addIdAndMetadata";
import { DatumData, DatumIdPayload } from "../../documentControl/DatumDocument";

describe("Field structure handling in addIdAndMetadata", () => {
  test("Simple field - no field structure in metadata", () => {
    // Create copy of data that includes field
    const data: DatumData = {
      field: "word",
      srcLang: "de",
      extra: "data_string",
    };

    const result = addIdAndMetadata(data, {
      idParts: ["test"],
    }) as DatumIdPayload;

    expect(result.data).toHaveProperty("field", "word");
    expect(result.meta.fieldStructure).toBeUndefined();
  });

  test("Composite field - sets fieldStructure in metadata", () => {
    const data: DatumData = {
      srcLang: "de",
      extra: "data_string",
      field: "word_%srcLang%",
    };

    const result = addIdAndMetadata(data, {
      idParts: ["test"],
    }) as DatumIdPayload;

    expect(result.data).toHaveProperty("field", "word_de");
    expect(result.meta.fieldStructure).toBe("word_%srcLang%");
  });

  test("Composite field - interpolates correctly", () => {
    const data: DatumData = {
      srcLang: "en",
      wordType: "noun",
      extra: "data_string",
    };

    const result = addIdAndMetadata(
      {
        ...data,
        field: "%wordType%_%srcLang%",
      },
      {
        idParts: ["test"],
      },
    ) as DatumIdPayload;

    expect(result.data).toHaveProperty("field", "noun_en");
    expect(result.meta.fieldStructure).toBe("%wordType%_%srcLang%");
  });

  test("No metadata mode - still sets field but not fieldStructure", () => {
    const data: DatumData = {
      srcLang: "de",
      extra: "data_string",
    };

    const result = addIdAndMetadata(
      {
        ...data,
        field: "word_%srcLang%",
      },
      {
        idParts: ["test"],
        noMetadata: true,
      },
    );

    // In no-metadata mode, field is a top-level property
    expect(result).toHaveProperty("field", "word_de");
    expect(result.meta).toBeUndefined();
  });
});
