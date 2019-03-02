import datetime as dt
from dateutil import parser as dtparse
import subprocess

iso_fmt = "%Y-%m-%d\t%H:%M:%S"
posix_fmt = "%H:%M:%S %Y-%m-%d"

now = dt.datetime.now().replace(microsecond=0)
posix_now = now.strftime(posix_fmt)


def midnight(qdate=dt.date.today()):
    # Gets the midnight datetime at the start of a given date

    if type(qdate) == str:
        return midnight(dt.datetime.strptime(qdate, "%Y-%m-%d"))

    if type(qdate) == dt.datetime:
        return dt.datetime.combine(qdate.date(), dt.time.min)

    if type(qdate) == dt.date:
        return dt.datetime.combine(qdate, dt.time.min)


def eod(qdate=dt.date.today()):
    # Gets the last possible time for a given date

    if type(qdate) == str:
        return eod(dt.datetime.strptime(qdate, "%Y-%m-%d"))

    if type(qdate) == dt.datetime:
        return dt.datetime.combine(qdate.date(), dt.time.max)

    if type(qdate) == dt.date:
        return dt.datetime.combine(qdate, dt.time.max)


def is_number(s):
    if type(s) in [float, int]:
        return True
    elif type(s) == str:
        return s.lstrip('-').replace('.', '', 1).isdigit()
    else:
        return False


def parse_duration(dur_str):
    # Parses phrases like "1week", "3 hours", etc. using POSIX-date's parser
    # I'm doing this for consistency with other programs in the interconnected system.
    # Returns a dt.timedelta object
    dur = dtparse.parse(subprocess.check_output(["date", "-d", posix_now + " + " + dur_str]), ignoretz=True) - now
    return dur


def parse_date(dt_str, reference=now):
    try:
        # Try to parse the string as an absolute datetime first
        return dtparse.parse(dt_str, ignoretz=True)

    except ValueError:
        # Otherwise parse as a duration relative to reference (e.g., 30 days, 5 months, etc.)
        delta_dur = parse_duration(dt_str)

        # Transform reference into a well formatted string if it isn't already
        if type(reference) != dt.datetime:
            reference = parse_date(reference)

        return reference + delta_dur


def human_dur_str(dur, printzero=False):
    # Print dur in this format: {}d{}h{}m{}s
    d = {"days": dur.days}
    d["hours"], rem = divmod(dur.seconds, 3600)
    d["minutes"], d["seconds"] = divmod(rem, 60)

    ret_str = ''
    if d["days"] > 0 or printzero:
        ret_str += str(d["days"]) + 'd'
    if d["hours"] > 0 or printzero:
        ret_str += str(d["hours"]) + 'h'
    if d["minutes"] > 0 or printzero:
        ret_str += str(d["minutes"]) + 'm'
    if d["seconds"] > 0 or printzero:
        ret_str += str(d["seconds"]) + 's'

    return ret_str
