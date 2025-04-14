import {
  DataOnlyPayload,
  DatumData,
  DatumMetadata,
  DatumPayload,
  EitherIdPayload,
  EitherPayload,
} from "../documentControl/DatumDocument";
import { AddCmdArgs } from "../commands/addCmd";
import { defaultIdComponents } from "../ids/defaultIdComponents";
import { buildIdStructure } from "../ids/buildIdStructure";
import { defaults } from "../input/defaults";
import { newHumanId } from "./newHumanId";
import { assembleId } from "../ids/assembleId";
import { IdError, FieldError } from "../errors";
import { toDatumTime } from "../time/datumTime";
import { now } from "../time/timeUtils";
import { interpolateFields } from "../utils/interpolateFields";

export function addIdAndMetadata<T>(
  data: DatumData<T>,
  args: Pick<
    AddCmdArgs,
    "noMetadata" | "idParts" | "idDelimiter" | "field" | "fieldless"
  >,
): EitherIdPayload<T> {
  let meta: DatumMetadata | undefined = undefined;
  if (!args.noMetadata) {
    meta = {
      humanId: newHumanId(),
    };

    // these will be overwritten later by addDoc, but useful to have them here
    // for undo and original id building
    meta.createTime = toDatumTime(now());
    meta.modifyTime = toDatumTime(now());
  }

  // Check if specified field parameter contains colon before processing
  if (args.field && args.field.includes(":")) {
    throw new FieldError(
      `Field cannot contain colons (:) as they are used as ID delimiters. Got: "${args.field}"`,
    );
  }

  // Process field if it contains % syntax
  if (args.field && args.field.includes("%")) {
    // Create a copy of data to avoid modifying the original
    data = { ...data };
    const processedField = interpolateFields({
      data,
      meta,
      format: args.field,
    });

    // Verify the processed field doesn't contain a colon
    if (processedField.includes(":")) {
      throw new FieldError(
        `Composite field cannot contain colons (:) as they are used as ID delimiters. Got: "${processedField}" from template "${args.field}"`,
      );
    }

    data.field = processedField;
  }

  // Check if field contains a colon, which would break ID parsing
  if (
    "field" in data &&
    data.field &&
    typeof data.field === "string" &&
    data.field.includes(":")
  ) {
    throw new FieldError(
      `Field cannot contain colons (:) as they are used as ID delimiters. Got: "${data.field}"`,
    );
  }

  const payload: EitherPayload<T> =
    meta !== undefined
      ? ({ data, meta } as DatumPayload<T>)
      : ({ ...data } as DataOnlyPayload<T>);

  const { defaultIdParts } = defaultIdComponents({
    data,
    meta,
  });

  // Build idStructure for the main part (without the field partition)
  const mainIdStructure = buildIdStructure({
    idParts: args.idParts ?? defaultIdParts,
    delimiter: args.idDelimiter ?? defaults.idDelimiter,
  });

  // Store the ID structure in metadata - this will be just the main part without field
  if (meta !== undefined && mainIdStructure.match(/(?<!\\)%/)) {
    meta.idStructure = mainIdStructure;
  }

  // Assemble the ID (assembleId now handles adding the field partition)
  const _id = assembleId({
    payload,
    idStructure: mainIdStructure,
  });

  if (_id === "") {
    throw new IdError("Provided or derived _id is blank");
  }

  const idPayload = { _id, ...payload };
  return idPayload;
}
