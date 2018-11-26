import pandas as pd
import numpy as np
import datetime as dt

import re
import os

from miscfunc import *

datum_dir=os.getenv("HOME")+'/Data/datum/'




class Datumdata:
    """
    Class for loading and processing datum data

    Data come in a few different varieties which are plotted differently
    All data entries have a timestamp (with date and time), and then an optional tab separated list of values
    The format of the extra data is listed here.

    Point data: simple data consisting of only a timestamp

    Block data: Blocks of time with a start and end
        'start' - start of a block
        'end'   - end of a block
        +N      - end of an N minute block
        -N      - end of an N minute whole inside of a block

    Category data: Discrete non-overlapping categories that take place until ended or another category begins
        <name>  - Start of a block of <name>, also ends the preceding block if it exists
        'end'   - end of a block
        -N      - end of an N minute whole inside of a block

    Pixel Coefficient (px_coeff) data: blocks whose width in pixels scales with their data
        V       - end of a block of time that is N pixels wide, where N = V * coeff

    Time Coefficient (tm_coeff) data: blocks whose width in minutes scales with their data
        V       - end of a block of time that is N minutes wide, where N = V * coeff
    """

    # TODO: Transition this whole thing to using dataframes more, like systematically building a dataframe with the calculated data
    def __init__(self, name, type=None, coeff=1):
        self.name = name
        self.type = type
        self.coeff = coeff

        # Memory of calculated timings and quantities (hashed by day)
        self.timings = {}
        self.minutes = {}

        # Any columns that specify units will store them in this dict
        self.units = {}

        # Initialize data variable. This is used to compare new data to old data when (re)loading data
        # If differences are found, the appropriate days will be flagged for recalculation in memory
        self.data = None

        self.load_data()

        self.process_header()

    def load_data(self):

        # Store a copy of the current data, to compare fr differences in the incoming data
        old_data = self.data

        # Load in the datum filecombining the date and time columns
        datum_file = datum_dir + self.name + '.tsv'
        # As raw text
        with open(datum_file, 'r') as f:
            self.raw_text = f.read()
        # As a pandas dataframe, with the date and time combined into one field, and set as the index
        data = pd.read_csv(datum_file, delimiter='\t', parse_dates=[[0, 1]], index_col=[0])

        # Convert the date_time column into datetime format
        data.index = pd.to_datetime(data.index)

        # Get into chronological order
        data.sort_index(inplace=True)

        # Data of 'block' type should have their Minutes column be strings (because of 'start' and 'end')
        # However, if no 'start' or 'end' exist, it will be read as a float. So convert.
        if self.type == 'block':
            data['Minutes'] = data['Minutes'].astype(str)

        # Search for new or changed entries, and flag that these should be recalculated rather than rely on the stored memory
        if old_data != None:
            new_or_changed = data[~data.isin(old_data).all(1)]
            # TODO: write file that removes entries for a day

        self.data = data


    def get_timings(self, date):
        if not date in self.timings:
            self.calculate_day(date)

        return self.timings[date]

    def get_minutes(self, date):
        if not date in self.minutes:
            self.calculate_day(date)

        return self.minutes[date]

    def calculate_day(self, date):
        """
        Data type independent function to get timings and totals for the day
        """

        # This class uses memory to avoid repetitious look ups of data
        # If data has already been calculated, don't recalculate
        if date != dt.date.today() and date in self.timings:
            return


        if self.type == 'block':
            day_timing = self.day_timing_blockdata(date)
            day_minutes = self.day_minutes_blockdata(day_timing)

        elif self.type == 'category':
            day_timing = self.day_timing_catdata(date)
            day_minutes = self.day_minutes_catdata(day_timing)

        elif self.type == 'point':
            date_str = date.strftime("%Y-%m-%d")
            try:
                day_timing = self.data[date_str].index
                day_minutes = 0
            except KeyError:
                day_timing = []
                day_minutes = 0

        else:
            raise RuntimeError('Data type is invalid or not supported yet')

        # Store the calculations for easy access later
        self.timings[date] = day_timing
        self.minutes[date] = day_minutes


    def day_timing_blockdata(self, date):
        """
        Gets the start and end times of each block on a given date
        :param date:
        :return day_blocks [a list of (start_time, end_time)]:
        """

        # For some reason, pandas needs time based indexing of time series to be a string
        date_str = date.strftime("%Y-%m-%d")

        # For block data, all entries are stored in the column, 'Minutes'
        # Load the data for the given date
        try:
            ts = self.data['Minutes'][date_str]
        except KeyError:
            # If no data exist for a given date, return empty list
            ts = pd.Series()

        # Initialize empty arrays to store the start times and end times
        start_times = []
        end_times = []

        # in_block tracks whether or not the data parser is inside of a start...end block
        # Use the previous entries in the data to determine how this day should start
        start_value = self.day_start_val(date)
        in_block = start_value

        # If starting in a block, add midnight to the start list
        if start_value:
            block_start = dt.datetime.combine(date, dt.time.min)
            start_times += [block_start]

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

                # Add this to the block dict
                start_times += [block_start]

                continue

            elif entry == 'end':
                # 'end' should never occur outside of a block
                if not (in_block):
                    # Special case, if end occurs as the first entry in the day, and the previous day is unknown, start a virtual block at midnight
                    if in_block == None:
                        block_start = timestamp.floor('D')
                        start_times += [block_start]
                    else:
                        raise ValueError('end cannot occur after another end: ' + str(timestamp) + str(entry))

                in_block = False
                block_end = timestamp

                # Add this to the block dict
                end_times += [block_end]

                continue

            elif entry == '[end]' or entry == '[start]':
                # These are commands relating to starting and stopping data collection as a whole just skip these dates
                continue

            elif is_number(entry):

                # Positive times can only be added outside of blocks because
                # work being done inside of a block is already being counted
                # Similarly, negative times can only occur inside blocks, representing
                # a "whole" in the block.
                mini_block_minutes = float(entry)

                if not ((mini_block_minutes >= 0) ^ bool(in_block)):
                    raise ValueError("Number not correctly associated with blocks\n{timestamp}, {entry}")

                # Assuming the number record was made at the end of when it occurred, calculate the beginning
                mini_block_start = timestamp - dt.timedelta(minutes=abs(mini_block_minutes))
                mini_block_end = timestamp

                # Add these to the appropriate arrays in day_blocks
                if mini_block_minutes >= 0:
                    start_times += [mini_block_start]
                    end_times += [mini_block_end]
                else:
                    start_times += [mini_block_end]
                    end_times += [mini_block_start]

                continue

            else:
                raise ValueError('Entry not properly formed:' + str(timestamp) + str(' , ') + str(entry))

        # Being within a block at the end of a day means count to end of day (a microsecond before midnight)
        # One exception to this is if the date is today, then the block extends to right now.
        if in_block:
            if date == pd.Timestamp.now().date():
                block_end = dt.datetime.now()
            else:
                block_end = dt.datetime.combine(date, dt.time(23,59,59,999999))

            end_times += [block_end]

        # Each block should have a start and an end, therefore start_times and end_times should be the same length
        if len(start_times) != len(end_times):
            raise RuntimeError('start and end arrays don\'t have the same length')

        # Zip the two lists together into a list of tuples, (start_time, end_time)
        return list(zip(start_times, end_times))

    def day_minutes_blockdata(self, day_timing):
        total_minutes = 0

        # Each block looks like (start, end), so subtract to find total length of each block
        for block in day_timing:
            total_minutes += (block[1] - block[0]).seconds / 60.0

        return total_minutes

    def day_timing_catdata(self, date):
        """
        Gets the start and end times for each category block on a given date
        :param date:
        :return cat_day_blocks { cat -> [list of (start_time, end_time)] }:
        """

        # For some reason, pandas needs time based indexing of time series to be a string
        date_str = date.strftime("%Y-%m-%d")

        # For category data, entries are stored in 2 columns
        # Load the data for the given date
        try:
            df = self.data.iloc[:,0:2][date_str]
        except KeyError:
            df = pd.DataFrame()

        # Initialize empty dicts to store the start times and end times for each category
        start_times = {}
        end_times = {}

        # in_block tracks whether or not the data parser is inside of a start...end block
        # Use the previous entries in the data to determine how this day should start
        start_value = self.day_start_val(date)
        in_block = start_value

        # If starting in a block, add midnight to the start list
        if start_value:
            block_start = dt.datetime.combine(date,dt.time())
            start_times.setdefault(start_value, []).append(block_start)

        # Iterate through entries on day
        for timestamp, col1, col2 in df.itertuples():

            # col1 can have 3 different values:
            # <category> - starts a block of that category (N.B. see col2 below)
            # end - ends the active category, or does nothing if no category is active
            # -N  - creates a whole of N minutes in the active category, ending at timestamp
            #
            # if col1 is <category>, then col2 can be:
            # NaN - starts a block of category
            # N - creates a category block that is N minutes long, ending at timestamp

            if col1 == 'start':
                raise ValueError('start should not occur in category file')

            elif col1 == 'end':
                if in_block:
                    # End the currently active block
                    end_times.setdefault(in_block, []).append(timestamp)
                    in_block = False

                # Don't do anything if not in a block
                continue

            elif col1 == '[end]' or col1 == '[start]':
                # These are commands relating to starting and stopping data collection as a whole just skip these
                continue

            if is_number(col1):
                # number must be negative in col1
                col1_num = float(col1)
                if col1_num > 0:
                    raise ValueError('Positive number in col1 not allowed')

                # Whole must occur inside of a block, if it is not inside a block, just ignore it
                if not(in_block):
                    next

                # Create a num sized whole in the active block
                whole_start = timestamp - dt.timedelta(minutes=abs(col1_num))
                whole_end = timestamp

                end_times.setdefault(in_block, []).append(whole_start)
                start_times.setdefault(in_block, []).append(whole_end)


            else:
                # Otherwise, treat the entry as a category name

                # Behavior in this instance is dependant on col2
                # If col2 is NaN, start a new category
                if np.isnan(col2):
                    # Same category should not be started twice in a row
                    if in_block == col1:
                        raise ValueError('Same category started twice in a row' + str(timestamp))

                    # End the current category
                    if in_block:
                        end_times.setdefault(in_block, []).append(timestamp)

                    # Start the new category
                    in_block = col1
                    start_times.setdefault(col1, []).append(timestamp)

                elif col2 >= 0:
                    # Create a block of size col2

                    mini_block_start = timestamp - dt.timedelta(minutes=col2)
                    mini_block_end = timestamp

                    # If another block is currently active, create a whole in it to fit this momentary block of another category
                    if in_block:
                        # Category should not be the same as the active
                        if in_block == col1:
                            raise ValueError('Category block cannot be created within itself' + str(timestamp))

                        end_times.setdefault(in_block, []).append(mini_block_start)
                        start_times.setdefault(in_block, []).append(mini_block_end)


                    start_times.setdefault(col1, []).append(mini_block_start)
                    end_times.setdefault(col1, []).append(mini_block_end)


                elif col2 < 0:
                    # Special case allowed where the category is listed with a negative number after it
                    # The project MUST match the currently active project though
                    if col1 == in_block:
                        # Create a num sized whole in the active block
                        whole_start = timestamp - dt.timedelta(minutes=abs(col1_num))
                        whole_end = timestamp

                        end_times.setdefault(in_block, []).append(whole_start)
                        start_times.setdefault(in_block, []).append(whole_end)

                else:
                    raise ValueError('Improperly formated category data')

        # Being within a block at the end of a day means count to end of day (a microsecond before midnight)
        # One exception to this is if the date is today, in which case the block extends to right now.
        if in_block:
            if date == pd.Timestamp.now().date():
                block_end = dt.datetime.now()
            else:
                block_end = dt.datetime.combine(date, dt.time(23, 59, 59, 999999))

            end_times.setdefault(in_block, []).append(block_end)

        # Final dict to return
        day_cats = {}

        # Check and zip all the start and end times together
        for category in start_times:

            # Each block should have a start and an end, therefore start_times and end_times should be the same length
            if len(start_times[category]) != len(end_times[category]):
                raise RuntimeError('start and end arrays don\'t have the same length')

            # Zip the two lists together into a list of tuples, (start_time, end_time)
            day_cats[category] = list(zip(start_times[category], end_times[category]))

        return day_cats

    def day_minutes_catdata(self, day_timing):
        total_minutes_by_cat = {}

        # day_timing is a hash with each category as its keys, each value is standard looking block data
        for cat in day_timing:
            total_minutes_by_cat[cat] = self.day_minutes_blockdata(day_timing[cat])

    def day_start_val(self, date):
        """
        Figure out how a given day should start and midnight when calculating its blocks

        How a day starts is dependent upon how the last day ended, so this function uses
        the value in the most recent entries to determine how to start the day.

        Returns True if within a block at midnight,
        False if not in a block
        <Category Name> if within a named category block.

        :param date:
        :return True,False, or Category:
        """

        # How a day starts is dependent upon how the last day ended,
        # Turn the date into a datetime at midnight
        midnight = dt.datetime.combine(date, dt.time())

        # Find the index of the entry most recent before the given date
        try:
            prev_ind = self.data.index.get_loc(midnight, method='ffill')
        except KeyError:
            # If no data exists before the day
            return False

        if self.type == 'block':
            while prev_ind >= 0:
                val = self.data.iloc[prev_ind]['Minutes']

                if val == 'start':
                    # In a generic block
                    return True

                if val == 'end':
                    # Not in any block
                    return False

                prev_ind -= 1

        elif self.type == 'category':
            while prev_ind >= 0:
                col1 = self.data.iloc[prev_ind,0]
                col2 = self.data.iloc[prev_ind,1]

                # Next starts off if category ended previous day
                if col1 == 'end':
                    return False

                # If the col1 is a not a number and col2 is also NaN, then next day starts in a category block.
                if (not is_number(col1)) and np.isnan(col2):
                    return col1

                prev_ind -= 1



        # If data is of another type, then assume not in a block
        return False

    def infer_data_type(self):
        """
        Examines the data to determine if it is
        Block data
        Category data
        or Timestamp data
        :return:
        """
        # TODO: make this function



