#!/usr/bin/env python3
from datumdata import Datumdata
import datetime as dt
import pandas as pd
from functools import reduce
import sys
from dateutil import parser

datumfile = "activity"
category = "work"

activity = Datumdata(name="activity", column="Activity", data_type="category")

# Determine number of hours worked this week
today = dt.date.today()
start_of_week = today - dt.timedelta(today.weekday())
if len(sys.argv) > 1:
    start_date = parser.parse(sys.argv[1])
else:
    start_date = start_of_week

if len(sys.argv) > 2:
    end_date = parser.parse(sys.argv[2])
else:
    end_date = today

days_to_calculate = pd.date_range(
    start=start_date, end=end_date, freq="D"
).to_pydatetime()


def dayhours(date):
    try:
        return activity.get_total(date)["work"] / 60
    except KeyError:
        return 0


hours_worked = map(lambda date: dayhours(date.date()), days_to_calculate)

print(reduce((lambda x, y: x + y), hours_worked))
