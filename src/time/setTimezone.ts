import { DateTime, Settings as DateTimeSettings } from "luxon";
import { BadTimezoneError } from "../errors";

function setTimezone(timezone?: string): number {
  if (timezone) {
    const tzNumber = Number(timezone);
    if (isNaN(tzNumber)) {
      // timezone is a named zone
      DateTimeSettings.defaultZone = timezone;
    } else {
      // timezone is a utc offset "+6"
      const tzStr = tzNumber >= 0 ? `+${tzNumber}` : `${tzNumber}`;
      DateTimeSettings.defaultZone = `UTC${tzStr}`;
    }
  } else {
    DateTimeSettings.defaultZone = "system";
  }
  const now = DateTime.local();
  if (!now.isValid) {
    throw new BadTimezoneError("timezone is invalid");
  }

  return now.offset / 60;
}

export default setTimezone;
