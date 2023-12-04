#!/usr/bin/env python3
import sys

from dayview import main as dayview
import datetime as dt

if __name__ == "__main__":
    print(f"sys argv: {sys.argv}")
    if len(sys.argv) < 2:
        raise ValueError("Usage: seeday <begin_date> [end_date]")
    begin_date = dt.date.fromisoformat(sys.argv[1])
    end_date = dt.date.fromisoformat(sys.argv[2]) if len(sys.argv) > 2 else begin_date
    n_days = (end_date - begin_date).days + 1
    dayview(
            imagename="seeday",
            begin_date=begin_date,
            end_date=end_date,
            xsize=1000,
    )