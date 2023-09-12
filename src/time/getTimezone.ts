import {
  FixedOffsetZone,
  IANAZone,
  Settings as DateTimeSettings,
  Zone,
} from "luxon";
import { BadTimezoneError } from "../errors";

export function getTimezone(timezone?: string | number): Zone {
  let zone: Zone;
  if (timezone) {
    if (typeof timezone === "number" || !isNaN(Number(timezone))) {
      zone = FixedOffsetZone.instance(Number(timezone) * 60);
    } else {
      zone = IANAZone.create(timezone);
    }
  } else {
    zone = DateTimeSettings.defaultZone as Zone;
  }
  if (!zone.isValid) {
    throw new BadTimezoneError("timezone is invalid");
  }

  return zone;
}
