import {
  FixedOffsetZone,
  IANAZone,
  Settings as DateTimeSettings,
  Zone,
} from "luxon";
import { BadTimezoneError } from "../errors";
import { defaultZone } from "./timeUtils";

export function getTimezone(timezone?: string | number): Zone | undefined {
  let zone: Zone;
  if (timezone !== undefined) {
    if (typeof timezone === "number" || !isNaN(Number(timezone))) {
      zone = FixedOffsetZone.instance(Math.round(Number(timezone) * 60));
    } else if (timezone === "system") {
      zone = defaultZone;
    } else {
      zone = IANAZone.create(timezone);
    }
  } else {
    return undefined;
  }
  if (!zone.isValid) {
    throw new BadTimezoneError("timezone is invalid");
  }

  return zone;
}
