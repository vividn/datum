#!/usr/bin/python3

# Workday indicator and analysis
# Shows the number of hours worked today
# In the future, this will be able to handle multiple days and have an automatically refreshing indicator

import pandas as pd
import datetime
import re

def count_time(ts):
    # If no entries exist for a day, 0 minutes were logged
    if len(ts) == 0:
        return 0.0

    total_minutes = 0.0
    in_block = None

    # day_begin is her

    # Iterate through entries on day
    for timestamp, entry in ts.iteritems():

        # Entries are either 'start', 'end', or a number.
        # 'start' and 'end' should form blocks of time whose minutes
        # should be counted towards the total number of minutes.
        # Numbers add directly to the total.

        if entry == 'start':
            # 'start' should never occur while already in a block
            if in_block:
                raise ValueError('start cannot occur after another start: ' + str(timestamp) + str(entry))

            in_block = True
            block_start = timestamp
            continue

        elif entry == 'end':
            # 'end' should never occur outside of a block
            if not(in_block):
                # Special case, if end occurs as the first entry in the day, start a virtual block at midnight
                if in_block == None:
                    block_start = timestamp.floor('D')
                else:
                    raise ValueError('end cannot occur after another end: ' + str(timestamp) + str(entry))

            in_block = False
            block_end = timestamp

            block_minutes = (block_end - block_start).seconds / 60.0
            total_minutes += block_minutes
            continue

        else:
            # Determine if entry is a number
            m = re.search('(-?)[0-9]+(\.[0-9]+)?', entry)

            if m:

                # Positive times can only be added outside of blocks because
                # work being done inside of a block is already being counted
                # Similarly, negative times can only occur inside blocks, representing
                # a short time during a work block that was spent not working.
                if bool(m.group(1)) ^ bool(in_block):
                    raise ValueError('Number not correctly associated with blocks')

                total_minutes += float(entry)
                continue

            else:
                raise ValueError('Entry not properly formed:' + str(timestamp) + str(' , ') + str(entry))

    # Being within a block at the end of a day means count to end of day (midnight)
    # One exception to this is if the date is today, then the block extends to right now.
    if in_block:
        if timestamp.date() == pd.Timestamp.now().date():
            block_end = timestamp.now()
        else:
            block_end = timestamp.ceil('D')

        block_minutes = (block_end - block_start).seconds / 60.0
        total_minutes += block_minutes

    # Finally, return the total number of minutes recorded on the day
    return total_minutes


# Load in the workday file, combining the date and time columns
workday_file = '/home/vividn/Data/datum/hw.tsv'
data = pd.read_csv(workday_file, delimiter='\t', parse_dates=[[0, 1]], index_col=[0])

# Convert the date_time column into datetime format
data.index = pd.to_datetime(data.index)

# Resample using a custom resample algorithm that includes ranges of minutes between start and end tags
ts = data['Minutes']
daily = ts.resample('D').apply(count_time)

print(daily / 60)
