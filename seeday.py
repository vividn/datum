#!/usr/bin/env python3
import sys
from subprocess import run

from dayview import main as dayview
import datetime as dt

if __name__ == "__main__":
    print(f"sys argv: {sys.argv}")
    if len(sys.argv) < 2:
        raise ValueError("Usage: seeday <begin_date> [end_date]")
    iso_begin = run(["date", "-d", sys.argv[1], '+%Y-%m-%d'], capture_output=True, text=True).stdout.strip()
    begin_date = dt.date.fromisoformat(iso_begin)
    if len(sys.argv) > 2:
        iso_end = run(["date", "-d", sys.argv[2], '+%Y-%m-%d'], capture_output=True, text=True).stdout.strip()
        end_date = dt.date.fromisoformat(iso_end)
    else:
        end_date = begin_date
    n_days = (end_date - begin_date).days + 1
    dayview(
            imagename="seeday",
            begin_date=begin_date,
            end_date=end_date,
            xsize=1000,
    )