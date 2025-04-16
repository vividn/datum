import { testDbLifecycle } from "../../__test__/test-utils";
import { addCmd } from "../../commands/addCmd";
import { setupCmd } from "../../commands/setupCmd";
import { updateCmd } from "../../commands/updateCmd";
import { DatumDocument } from "../../documentControl/DatumDocument";

describe("simple and complex fields", () => {
  const dbName = "field_structure_test";
  testDbLifecycle(dbName);

  beforeEach(async () => {
    await setupCmd("");
  });

  it("adds a simple field without setting fieldStructure", async () => {
    const doc = await addCmd("word srcLang=de extra=data_string --id test");
    expect(doc).toHaveProperty("data.field", "word");
    expect(doc).toHaveProperty("data.srcLang", "de");
    expect(doc).toHaveProperty("data.extra", "data_string");
    expect((doc as DatumDocument).meta).not.toHaveProperty("fieldStructure");
  });

  it("adds a composite field with fieldStructure in metadata", async () => {
    const doc = await addCmd(
      "word_%srcLang% srcLang=de extra=data_string --id test",
    );
    expect(doc).toHaveProperty("data.field", "word_de");
    expect(doc).toHaveProperty("data.srcLang", "de");
    expect(doc).toHaveProperty("data.extra", "data_string");
    expect((doc as DatumDocument).meta).toHaveProperty(
      "fieldStructure",
      "word_%srcLang%",
    );
  });

  it("updates a composite field value when data changes", async () => {
    // Create document with composite field
    const originalDoc = await addCmd(
      "word_%srcLang% srcLang=de extra=data_string --id test",
    );
    expect(originalDoc).toHaveProperty("data.field", "word_de");

    // Update srcLang which should update the field
    const updatedDocs = await updateCmd([originalDoc._id, "srcLang=fr"]);
    expect(updatedDocs).toHaveLength(1);
    const updatedDoc = updatedDocs[0];

    // Check that field was recalculated based on the template in fieldStructure
    expect(updatedDoc).toHaveProperty("data.field", "word_fr");
    expect(updatedDoc).toHaveProperty("data.srcLang", "fr");
    expect((updatedDoc as DatumDocument).meta).toHaveProperty(
      "fieldStructure",
      "word_%srcLang%",
    );
  });

  it("allows changing from simple field to composite field", async () => {
    // Create document with simple field
    const originalDoc = await addCmd("simple srcLang=de --id test");
    expect(originalDoc).toHaveProperty("data.field", "simple");
    expect((originalDoc as DatumDocument).meta).not.toHaveProperty(
      "fieldStructure",
    );

    // Update to composite field
    const updatedDocs = await updateCmd(
      `${originalDoc._id} field=%srcLang%_field`,
    );
    expect(updatedDocs).toHaveLength(1);
    const updatedDoc = updatedDocs[0];

    // Check that field was calculated based on template and fieldStructure was added
    expect(updatedDoc).toHaveProperty("data.field", "de_field");
    expect((updatedDoc as DatumDocument).meta).toHaveProperty(
      "fieldStructure",
      "%srcLang%_field",
    );
  });

  it("allows changing from composite field to simple field", async () => {
    // Create document with composite field
    const originalDoc = await addCmd("word_%srcLang% srcLang=de --id test");
    expect(originalDoc).toHaveProperty("data.field", "word_de");
    expect((originalDoc as DatumDocument).meta).toHaveProperty(
      "fieldStructure",
    );

    // Update to simple field
    const updatedDocs = await updateCmd(`${originalDoc._id} field=simple`);
    expect(updatedDocs).toHaveLength(1);
    const updatedDoc = updatedDocs[0];

    // Check that field was set directly and fieldStructure was removed
    expect(updatedDoc).toHaveProperty("data.field", "simple");
    expect((updatedDoc as DatumDocument).meta).not.toHaveProperty(
      "fieldStructure",
    );
  });

  it("recalculates field from complex composite structure when data changes", async () => {
    // Create document with complex composite field
    const originalDoc = await addCmd(
      "%type%_%srcLang%_%extra% type=word srcLang=de extra=test --id test",
    );
    expect(originalDoc).toHaveProperty("data.field", "word_de_test");

    // Update multiple values that affect the field
    const updatedDocs = await updateCmd(
      `${originalDoc._id} srcLang=fr type=noun extra=complex`,
    );
    expect(updatedDocs).toHaveLength(1);
    const updatedDoc = updatedDocs[0];

    // Check that field was recalculated with all new values
    expect(updatedDoc).toHaveProperty("data.field", "noun_fr_complex");
    expect(updatedDoc).toHaveProperty("data.srcLang", "fr");
    expect(updatedDoc).toHaveProperty("data.type", "noun");
    expect(updatedDoc).toHaveProperty("data.extra", "complex");
    expect((updatedDoc as DatumDocument).meta).toHaveProperty(
      "fieldStructure",
      "%type%_%srcLang%_%extra%",
    );
  });
});
