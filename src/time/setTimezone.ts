import { DateTime, FixedOffsetZone, IANAZone, Settings as DateTimeSettings, Zone } from "luxon";
import { BadTimezoneError } from "../errors";

function setZone(timezone?: string): Zone {
  let zone: Zone;
  if (timezone) {
    const tzNumber = Number(timezone);
    if (isNaN(tzNumber)) {
      // timezone is a named zone
      zone = IANAZone.create(timezone);
    } else {
      // timezone is a utc offset "+6"
      zone = FixedOffsetZone.instance(tzNumber * 60);
    }
  } else {
    zone = DateTimeSettings.defaultZone as Zone;
  }
  if (!zone.isValid) {
    throw new BadTimezoneError("timezone is invalid");
  }

  return zone;
}

export default setZone;
