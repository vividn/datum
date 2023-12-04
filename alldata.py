#!/usr/bin/env python3
from dayview import main as dayview
import datetime as dt

if __name__ == "__main__":
    dayview(
            imagename="allData",
            begin_date=dt.date(2016,11,27),
            end_date=dt.date.today(),
            xsize=1000,
            ysize=300000
    )