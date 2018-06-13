#!/usr/bin/python3

# Workday indicator and analysis
# Shows the number of hours worked today
# In the future, this will be able to handle multiple days and have an automatically refreshing indicator

import pandas as pd
import datetime


def count_time(ts):
    # If no entries exist for a day, 0 minutes were logged
    if len(ts) == 0:
        return 0

    total_minutes = 0
    in_block = False

    # Iterate through entries on day
    for timestamp, entry in ts.iteritems():

        # Entries are either 'start', 'end', or a number.
        # 'start' and 'end' should form blocks of time whose minutes
        # should be counted towards the total number of minutes.
        # Numbers add directly to the total.

        if entry == 'start':
            in_block = True
            block_start = timestamp

        elif entry == 'end':
            in_block = False
            block_end = timestamp

        # Number
        if entry.isdigit():
            total_minutes += int(entry)

        else:
            print(entry)
            # TODO: start, stop, throw error if something else.
            # TODO: caught errors should resolve as X

    print(type(ts))


# Load in the workday file, combining the date and time columns
workday_file = '/home/vividn/Data/datum/workday.tsv'
data = pd.read_csv(workday_file, delimiter='\t', parse_dates=[[0, 1]], index_col=[0])

# Convert the date_time column into datetime format
data.index = pd.to_datetime(data.index)

# Resample using a custom resample algorithm that includes ranges of minutes between start and end tags
ts = data.resample('D').apply(count_time)
# print(ts)
