#!python3
from datumdata import Datumdata
import datetime as dt
import pandas as pd
from functools import reduce

datumfile = "activity"
category = "work"

activity = Datumdata(name="activity", column="Activity", data_type="category")

# Determine number of hours worked this week
today = dt.date.today()
start_of_week = today - dt.timedelta(today.weekday())

days_to_calculate = pd.date_range(start=start_of_week, end=today, freq='D').to_pydatetime()

def dayhours(date):
    try:
        return activity.get_total(date)['work'] / 60
    except KeyError:
        return 0

hours_worked = map(lambda date: dayhours(date.date()), days_to_calculate)

print(reduce((lambda x, y: x+y), hours_worked))
