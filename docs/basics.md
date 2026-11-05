# The basics

What datum.day's tutorial said before its games, kept here for those who read before they play.

## First of all: why keep data?

Data are the path to truth. Memory by itself is error-prone. It keeps what stood out and smooths over the rest. It
degrades over time, and is heavily influenced by how things are right now. Instead, by logging things as they happen,
you maintain a record that persists. Later during review you can understand where the time goes, how things actually
were, and notice the patterns. Furthermore, the act of actively tracking data forces you to pay attention, which helps
build understanding and acceptance.

Datum is a power tool for collecting and analyzing your life data. So it is important to first understand some key
concepts and how to use the tool effectively.

## Important terminology

| term  | meaning |
| ----- | ------- |
| field | one thing you track and pay attention to (sleep, mood, activity) |
| entry | a timestamped log documenting the field in your life |
| state | Persistent attribute of a field. Entries can modify a field's state, which lasts until the next state change |
| dur   | Rather than persistently changing a field's state, an entry can specify a duration, which describes how long the field was just in that state, before returning to its previous state |

## Kinds of data

Data come in a few kinds, by how they relate to time:

- ● **moment**: something that happens or is measured at a single point in time. Represented by dots on a
  timeline. Moments have no state and no duration, but can store extra data. Ex. medication, alarm, weight, coffee
- █ **on/off**: on/off data have just 2 states, true and false. Represented by blocks of a single color on the
  timeline. For things that happen for a while and then stop happening. Ex. sleep, screentime, work
- ▞ **change**: change data use custom states to describe things that pass through different phases or modes.
  Represented by blocks that change color on the timeline as they change states. Ex. environment, project, activity,
  stoplight
- ¢ **record**: a collection of keys and values with no relation to time. Used for storing information about the world.
  Ex. people, words, catalogs
- ▤ **all day**: a value that belongs to a whole day rather than to a moment in it. Every entry is on a date with no
  time: today, or the day `-d` names (`datum allday steps 8000`, `datum allday steps 9000 -d 2026-11-01`), and `-t`
  is refused. The value is that day's alone; the next day does not inherit it. Ex. steps, a day's rating, a daily total

The developer's model, from an entry to tracked fields, is [concepts.md](concepts.md).
