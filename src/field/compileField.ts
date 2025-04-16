import {
  DatumData,
  DatumMetadata,
  EitherPayload,
  isDatumPayload,
} from "../documentControl/DatumDocument";
import { FieldError } from "../errors";
import { interpolateFields } from "../utils/interpolateFields";

export function compileField(
  payload: EitherPayload,
  fieldStructure?: string,
): void {
  let data;
  let meta: DatumMetadata | undefined;
  if (isDatumPayload(payload)) {
    data = payload.data as DatumData;
    meta = payload.meta;
  } else {
    data = payload as DatumData;
  }

  fieldStructure ??= meta?.fieldStructure ?? data.field;

  // Process field if it contains % syntax
  if (fieldStructure?.includes("%")) {
    const processedField = interpolateFields({
      data,
      meta,
      format: fieldStructure,
    });

    // Verify the processed field doesn't contain a colon
    if (processedField.includes(":")) {
      throw new FieldError(
        `Composite field cannot contain colons (:) as they are used as ID delimiters. Got: "${processedField}" from template "${fieldStructure}"`,
      );
    }

    // Store fieldStructure in metadata and the processed value in data.field
    data.field = processedField; // Use the interpolated value, not the template
    if (meta !== undefined) {
      meta.fieldStructure = fieldStructure;
    }
  } else {
    // If fieldStructure is not a template, store it directly in data.field
    if (meta !== undefined) {
      delete meta.fieldStructure;
    }
    data.field = fieldStructure;
  }

  // Check if field contains a colon, which would break ID parsing
  if ("field" in data && data.field && data.field.includes(":")) {
    throw new FieldError(
      `Field cannot contain colons (:) as they are used as ID delimiters. Got: "${data.field}"`,
    );
  }
}
