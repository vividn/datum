describe("choresView", () => {
  describe("map", () => {
    it.todo("emits nothing if there is no nextTime or nextDate");
    it.todo("emits nothing if there is no field");
    it.todo("emits occurTime as time and lastOccur if it exists");
    it.todo(
      "emits createTime as time if occurTime does not exist, with lastOccur set to zeroDate",
    );
    it.todo("sets next to be nextDate with just nextDate");
    it.todo("sets next to be nextTime with just nextTime");
    it.todo("combines nextDate and nextTime together if both are there");
    it.todo("still emits next if there is just createTime");
    it.todo("generates an ITI with the proper integral part if next is a date");
    it.todo(
      "uses the percentage through the day of occurrence as the decimal part of ITI if next is a date",
    );
    it.todo("uses the exact ITI if next is a time");
    it.todo("does not emit an ITI if there is no occurTime");
  });

  describe("reduce", () => {
    it.todo("combines the values to determine the latest time, next, lastOccur, and iti");
  });
});
