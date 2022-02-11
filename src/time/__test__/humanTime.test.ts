import { DateTime, Settings } from "luxon";

const mockNow = DateTime.utc(2022, 2, 11, 9, 20, 0);

beforeEach(async () => {
  Settings.now = () => mockNow.toMillis();
});


it.todo("displays HH:mm:ss if the DateTime is today");

it.todo("displays -1d HH:mm:ss if yesterday");

it.todo("displays +1d HH:mm:ss if tomorrow");

it.todo("displays MMM d, HH:mm:ss if same year");

it.todo("displays yyyy-MM-dd, HH:mm:ss if different year");

it.todo("adds UTC+N to end if utc offset does not match locale");

it.todo("does not add UTC+N if the offset is the same as locale, even if the timezone is technically different");

it.todo("still thinks it's today even if utc date is different than local date");
