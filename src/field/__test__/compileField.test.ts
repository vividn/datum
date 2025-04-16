import { compileField } from "../compileField";
import { FieldError } from "../../errors";
import {
  DatumData,
  DatumMetadata,
  DatumPayload,
} from "../../documentControl/DatumDocument";

describe("compileField function", () => {
  test("uses existing fieldStructure in metadata if available", () => {
    // Create test document with existing fieldStructure in metadata
    const data: DatumData = {
      category: "test",
      language: "en",
    };
    const meta: DatumMetadata = {
      fieldStructure: "%category%_%language%",
    };
    const payload: DatumPayload = { data, meta };

    // Run compileField without providing a new field value
    compileField(payload);

    // Should use the existing fieldStructure to interpolate field
    expect(payload.data.field).toBe("test_en");
    expect(payload.meta.fieldStructure).toBe("%category%_%language%");
  });

  test("explicit fieldStructure parameter overrides both existing field and metadata.fieldStructure", () => {
    // Create test document with existing field and fieldStructure
    const data: DatumData = {
      field: "existing_field",
      category: "test",
      language: "en",
    };
    const meta: DatumMetadata = {
      fieldStructure: "%category%_%language%",
    };
    const payload: DatumPayload = { data, meta };

    // Run compileField with explicit fieldStructure parameter
    compileField(payload, "new_%language%");

    // Should use the provided fieldStructure
    expect(payload.data.field).toBe("new_en");
    expect(payload.meta.fieldStructure).toBe("new_%language%");
  });

  test("updates field when related data changes with existing fieldStructure", () => {
    // Create test document with existing fieldStructure
    const data: DatumData = {
      field: "test_en", // Initial field value
      category: "test",
      language: "en",
    };
    const meta: DatumMetadata = {
      fieldStructure: "%category%_%language%",
    };
    const payload: DatumPayload = { data, meta };

    // Change the language property
    payload.data.language = "fr";

    // Run compileField to update field based on existing fieldStructure
    compileField(payload);

    // Field should be updated based on new language value
    expect(payload.data.field).toBe("test_fr");
    expect(payload.meta.fieldStructure).toBe("%category%_%language%");
  });

  test("removes fieldStructure from metadata when passing a non-template field", () => {
    // Create test document with existing fieldStructure
    const data: DatumData = {
      category: "test",
      language: "en",
    };
    const meta: DatumMetadata = {
      fieldStructure: "%category%_%language%",
    };
    const payload: DatumPayload = { data, meta };

    // Run compileField with a non-template string
    compileField(payload, "static_field");

    // Should keep the static field and remove fieldStructure
    expect(payload.data.field).toBe("static_field");
    expect(payload.meta.fieldStructure).toBeUndefined();
  });

  test("updates meta.fieldStructure when it's a static value", () => {
    // Create test document with existing fieldStructure that is static
    const data: DatumData = {
      category: "test",
      language: "en",
    };
    const meta: DatumMetadata = {
      fieldStructure: "static_field",
    };
    const payload: DatumPayload = { data, meta };

    // Run compileField
    compileField(payload);

    // Should use the static field from fieldStructure
    expect(payload.data.field).toBe("static_field");
    // Since it's not a template, fieldStructure should be removed
    expect(payload.meta.fieldStructure).toBeUndefined();
  });

  test("throws error if interpolated field contains a colon", () => {
    // Create test document with data that will cause a colon in the field
    const data: DatumData = {
      category: "test:invalid",
      language: "en",
    };
    const meta: DatumMetadata = {
      fieldStructure: "%category%_%language%",
    };
    const payload: DatumPayload = { data, meta };

    // Run compileField should throw FieldError
    expect(() => compileField(payload)).toThrow(FieldError);
    expect(() => compileField(payload)).toThrow(/cannot contain colons/);
  });

  test("can use metadata fields in field templates", () => {
    // Create test document with metadata that should be interpolated into field
    const data: DatumData = {
      someData: "data-value",
    };
    const meta: DatumMetadata = {
      humanId: "test-human-id",
      fieldStructure: "%?humanId%_prefix",
    };
    const payload: DatumPayload = { data, meta };

    // Run compileField to interpolate metadata into field
    compileField(payload);

    // Check field is interpolated with metadata value
    expect(payload.data.field).toBe("test-human-id_prefix");
    expect(payload.meta.fieldStructure).toBe("%?humanId%_prefix");
  });
});
