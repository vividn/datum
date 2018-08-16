import pandas as pd
import numpy as np
import datetime as dt
import re
import os

datum_dir=os.getenv("HOME")+'/Data/datum/'


def is_number(s):
    return s.lstrip('-').replace('.', '', 1).isdigit()


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

    def __init__(self, name, type=None, coeff=1):
        self.name = name
        self.type = type
        self.coeff = coeff

        # Load in the datum file, combining the date and time columns
        datum_file = datum_dir + name + '.tsv'
        self.data = pd.read_csv(datum_file, delimiter='\t', parse_dates=[[0, 1]], index_col=[0])

        # Convert the date_time column into datetime format
        self.data.index = pd.to_datetime(self.data.index)

        # Get into chronological order
        self.data.sort_index(inplace=True)

    def get_day(self, date):
        """
        Data type independent function to get blocks for the day
        """
        if self.type == 'block':
            return self.get_day_blocks(date)

        elif self.type == 'category':
            return self.get_day_cats(date)

        elif self.type == 'point':
            date_str = date.strftime("%Y-%m-%d")
            return self.data[date_str].index

        else:
            raise RuntimeError('Data type is invalid or not supported yet')

    def get_day_blocks(self, date):
        """
        Gets the start and end times of each block on a given date
        :param date:
        :return day_blocks [a list of (start_time, end_time)]:
        """

        # For some reason, pandas needs time based indexing of time series to be a string
        date_str = date.strftime("%Y-%m-%d")

        # For block data, all entries are stored in the column, 'Minutes'
        # Load the data for the given date
        ts = self.data['Minutes']
        try:
            ts_date = ts[date_str]
        except KeyError:
            # If no data exist for a given date, return empty list
            return []

        # Initialize empty arrays to store the start times and end times
        start_times = []
        end_times = []

        # in_block tracks whether or not the data parser is inside of a start...end block
        # Use the previous entries in the data to determine how this day should start
        start_value = self.day_start_val(date)
        in_block = start_value

        # If starting in a block, add midnight to the start list
        if start_value:
            block_start = ts_date.index[0].floor('D')
            start_times += [block_start]

        # Iterate through entries on day
        for timestamp, entry in ts_date.iteritems():

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
            if date.date() == pd.Timestamp.now().date():
                block_end = dt.datetime.now()
            else:
                block_end = dt.datetime.combine(date, dt.time(23,59,59,999999))

            end_times += [block_end]

        # Each block should have a start and an end, therefore start_times and end_times should be the same length
        if len(start_times) != len(end_times):
            raise RuntimeError('start and end arrays don\'t have the same length')

        # Zip the two lists together into a list of tuples, (start_time, end_time)
        return zip(start_times, end_times)

    def get_day_cats(self, date):
        """
        Gets the start and end times for each category block on a given date
        :param date:
        :return cat_day_blocks { cat -> [list of (start_time, end_time)] }:
        """

        # For some reason, pandas needs time based indexing of time series to be a string
        date_str = date.strftime("%Y-%m-%d")

        # For category data, entries are stored in 2 columns
        # Load the data for the given date
        df = self.data.iloc[:,0:2][date_str]

        # Initialize empty dicts to store the start times and end times for each category
        start_times = {}
        end_times = {}

        # in_block tracks whether or not the data parser is inside of a start...end block
        # Use the previous entries in the data to determine how this day should start
        start_value = self.day_start_val(date)
        in_block = start_value

        # If starting in a block, add midnight to the start list
        if start_value:
            block_start = df.index[0].floor('D')
            start_times[start_value] += [block_start]

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
                    end_times[in_block] += [timestamp]
                    in_block = False

                # Don't do anything if not in a block
                continue

            elif col1 == '[end]' or col2 == '[start]':
                # These are commands relating to starting and stopping data collection as a whole just skip these
                continue

            if is_number(col1):
                # number must be negative in col1
                col1_num = float(col1)
                if col1_num > 0:
                    raise ValueError('Positve number in col1 not allowed')

                # Whole must occur inside of a block
                if not(in_block):
                    raise ValueError('Whole can only occur inside an active block')

                # Create a num sized whole in the active block
                whole_start = timestamp - dt.timedelta(minutes=abs(col1_num))
                whole_end = timestamp

                end_times[in_block] += [whole_start]
                start_times[in_block] += [whole_end]


            else:
                # Otherwise, treat the entry as a category name

                # Behavior in this instance is dependant on col2
                # If col2 is NaN, start a new category
                if np.isnan(col2):
                    # Same category should not be started twice in a row
                    if in_block == col1:
                        raise ValueError('Same category started twice in a row')

                    # End the current category
                    if in_block:
                        end_times[in_block] += [timestamp]

                    # Start the new category
                    in_block = col1
                    start_times[col1] += [timestamp]

                elif col2 >= 0:
                    # Create a block of size col2 in the middle the currently active block and then resume the active block

                    # Category should not be the same as the active
                    if in_block == col1:
                        raise ValueError('Category block cannot be created within itself')

                    mini_block_start = timestamp - dt.timedelta(minutes=col2)
                    mini_block_end = timestamp

                    end_times[in_block] += [mini_block_start]
                    start_times[col1] += [mini_block_start]
                    end_times[col1] += [mini_block_end]
                    start_times[in_block] += [mini_block_end]

                elif col2 < 0:
                    # Special case allowed where the category is listed with a negative number after it
                    # The project MUST match the currently active project though
                    if col1 == in_block:
                        # Create a num sized whole in the active block
                        whole_start = timestamp - dt.timedelta(minutes=abs(col1_num))
                        whole_end = timestamp

                        end_times[in_block] += [whole_start]
                        start_times[in_block] += [whole_end]

                else:
                    raise ValueError('Improperly formated category data')

        # Being within a block at the end of a day means count to end of day (a microsecond before midnight)
        # One exception to this is if the date is today, then the block extends to right now.
        if in_block:
            if date.date() == pd.Timestamp.now().date():
                block_end = dt.datetime.now()
            else:
                block_end = dt.datetime.combine(date, dt.time(23, 59, 59, 999999))

            end_times[in_block] += [block_end]

        # Final dict to return
        day_cats = {}

        # Check and zip all the start and end times together
        for category in start_times:

            # Each block should have a start and an end, therefore start_times and end_times should be the same length
            if len(start_times[category]) != len(end_times[category]):
                raise RuntimeError('start and end arrays don\'t have the same length')

            # Zip the two lists together into a list of tuples, (start_time, end_time)
            day_cats[category] = zip(start_times[category], end_times[category])

        return day_cats

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
        midnight = dt.combine(date, dt.time())

        # Find the index of the entry most recent before the given date
        prev_ind = self.data.index.get_loc(midnight, method='')
        while prev_ind >= 0:
            val = self.data[prev_ind]

            if val == 'start':
                # In a generic block
                return True

            if val == 'end':
                # Not in any block
                return False

            # If the value is a not a number, then next day starts in a category block.
            if not (re.match('^-?[0-9]', val)):
                return val

            prev_ind -= 1

        # If no data exists before the day, then assume not in a block
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


