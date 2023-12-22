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
import { IdError } from "../errors";
import { now, toDatumTime } from "../time/timeUtils";

export function addIdAndMetadata<T>(
  data: DatumData<T>,
  args: Pick<AddCmdArgs, "noMetadata" | "idPart" | "idDelimiter" | "partition">,
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
  const payload: EitherPayload<T> =
    meta !== undefined
      ? ({ data, meta } as DatumPayload<T>)
      : ({ ...data } as DataOnlyPayload<T>);
  const { defaultIdParts, defaultPartitionParts } = defaultIdComponents({
    data,
  });

  const idStructure = buildIdStructure({
    idParts: args.idPart ?? defaultIdParts,
    delimiter: args.idDelimiter ?? defaults.idDelimiter,
    partition: args.partition ?? defaultPartitionParts,
  });

  // don't include idStructure if it is just a raw string (i.e. has no field references in it)
  // that would be a waste of bits since _id then is exactly the same
  if (meta !== undefined && idStructure.match(/(?<!\\)%/)) {
    meta.idStructure = idStructure;
  }

  const _id = assembleId({
    payload,
    idStructure,
  });
  if (_id === "") {
    throw new IdError("Provided or derived _id is blank");
  }
  const idPayload = { _id, ...payload };
  return idPayload;
}
