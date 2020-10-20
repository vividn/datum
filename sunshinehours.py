#!python3
from datumdata import Datumdata
import datetime as dt
import pandas as pd
from functools import reduce

datumfile = "activity"
column = "Activity"
category = "sunshine"

activity = Datumdata(name=datumfile, column=column, data_type="category")

# Determine number of hours worked this week
today = dt.date.today()
start_date = dt.date(2020,3,20)

days_to_calculate = pd.date_range(
    start=start_date, end=today, freq="D"
).to_pydatetime()


def dayhours(date):
    try:
        return activity.get_total(date)[category] / 60
    except KeyError:
        return 0


hours_worked = map(lambda date: dayhours(date.date()), days_to_calculate)

print(reduce((lambda x, y: x + y), hours_worked))
