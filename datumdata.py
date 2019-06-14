import pandas as pd
import numpy as np
import datetime as dt

import re
import os

from miscfunc import *

datum_dir = os.getenv("HOME") + '/Data/datum/'


class Datumdata:
    """
    Class for loading and processing datum data

    Data come in a few different varieties which are plotted differently
    All data entries have a timestamp (with date and time), and then an optional tab separated list of values
    The format of the extra data is listed here.

    Point data: totals reflect a count of timestamped entries

    Sum data: totals are calculated by summing the data

    Mean data: totals are calculated by averaging the data TODO

    Block data: Blocks of time with a start and end
        'start' - start of a block
        'end'   - end of a block
        +N      - end of an N minute block
        -N      - end of an N minute whole inside of a block

    Category data: Discrete non-overlapping categories that take place until ended or another category begins
        <name>  - Start of a block of <name>, also ends the preceding block if it exists
        'end'   - end of a block
        -N      - end of an N minute whole inside of a block
    """

    # TODO: Transition this whole thing to using dataframes more, \
    #  like systematically building a dataframe with the calculated data
    def __init__(self, name, column=None, data_type=None):
        # As a shorthand for column, name can have column listed after a colon, e.g., 'activity:project'
        if len(name.split(':', maxsplit=1)) == 2 and column is None:
            self.name, self.column = name.split(':', maxsplit=1)
        else:
            self.name = name
            self.column = column

        # Ensure column starts with capital letter
        if self.column is not None:
            self.column = self.column.title()

        # Set dataType if explicitly given (will be automatically inferred later if it is not)
        self.data_type = data_type

        # Memory of calculated timings and quantities (hashed by day)
        self.timings = {}
        self.totals = {}

        # Any columns that specify units or defaults will store them in these dicts
        self.units = {}
        self.defaults = {}

        # Initialize data variable. This is used to compare new data to old data when (re)loading data
        # If differences are found, the appropriate days will be flagged for recalculation in memory
        self.raw_text = None
        self.data = None

        # Load in the data
        self.datum_file = datum_dir + self.name + '.tsv'
        self.load_data()

    def load_raw(self):
        with open(self.datum_file, 'r') as f:
            self.raw_text = f.read()

    def load_data(self):

        # Store a copy of the current data, to compare fr differences in the incoming data
        old_data = self.data

        # Load in the raw text of the datum file
        self.load_raw()

        # Make pandas dataframe from datum file, with the date and time combined into one field and set as the index
        self.data = pd.read_csv(self.datum_file, delimiter='\t', parse_dates=[[0, 1]], index_col=[0])

        # Ensure the date_time column is in datetime format
        self.data.index = pd.to_datetime(self.data.index)

        # Process the header to get appropriate column names (as well as primary column, units, etc.)
        self.process_header()

        # Get into chronological order
        self.data.sort_index(inplace=True)

        # If no column has been designated either explicitly or via the header,
        # then use:
        # The first column after Date and Time
        # or, the "Time" column if only "Date" and "Time" exist
        if self.column is None:
            if len(self.data.columns) > 0:
                self.column = self.data.columns[0]
            else:
                self.column = 'Time'

        # Infer data type based on contents of column, if not explicitly given
        if self.data_type is None:
            self.infer_type()

        # Data of 'block' type should have their Minutes column be strings (because of 'start' and 'end')
        # However, if no 'start' or 'end' exist, it will be read as a float. So convert.
        if self.data_type == 'block':
            self.data['Minutes'] = self.data['Minutes'].astype(str)

        # If reloading data, flag new or changed entries to be recalculated rather than rely on the stored memory
        if old_data is not None:
            new_or_changed = self.data[~self.data.isin(old_data).all(1)]

            # Delete the stored memory for all the days with changes
            for date_with_changes in new_or_changed.resample('D').index:
                self.timings[date_with_changes] = None
                self.totals[date_with_changes] = None

    def process_header(self):
        """
        The header of datum files can communicate a lot of extra information which can be useful in analyzing
        For a given column name, it can look like this

        *NAME(units)[default]

        all are optional except NAME, the name of the column

        * - represents the primary column for analysis.
            *Date - Will be counted as true if any entry is made on the date, type=date TODO
            *Time - Shorthand to just count the entries, type='point'
            *Minutes - type='block'
            *{others} =

        (units) - for specifying units of the column, is stored in self.units["Column"]
        [default] - what to fill in for blank spaces in the column

        :return:
        """
        # TODO: Use symbols to indicate proper combination method

        # Split the header into an array of strings, the title of each column
        header = self.raw_text.split("\n", 1)[0].split("\t")
        if header[0] != "Date":
            raise RuntimeError('No header for "' + self.name + '". Please create one')

        header_re = re.compile(r'^(\*)?([^[(]+)((\[[^\]]*\])|(\([^)]*\))){0,2}$')
        for col in header:
            m = header_re.match(col)
            if not m:
                raise ValueError('Badly named column in data. {}: {}'.format(self.name, col))

            # Rename column to base name in dataframe
            base_name = m[2].title()
            self.data = self.data.rename(columns={m[0]: base_name})

            # Set primary column if it has not been set yet
            if m[1] and self.column is None:
                self.column = base_name

            # Set default '[]'
            if m[4]:
                col_default = m[4][1:-1]
                # Convert to float if appropriate
                try:
                    self.defaults[base_name] = float(col_default)
                except ValueError:
                    self.defaults[base_name] = col_default

            # Set units '()'
            if m[5]:
                self.units[base_name] = m[5][1:-1]

    def infer_type(self):
        if self.column is None:
            raise RuntimeError('Column must be defined before data type can be inferred')

        elif self.column == 'Date':
            self.data_type = 'bool'

        elif self.column == 'Time':
            self.data_type = 'point'

        elif self.column == 'Minutes':
            self.data_type = 'block'

        else:
            # If column consists of numbers, sum by default
            if self.data[self.column].dtype in ['float64', 'int64']:
                self.data_type = 'sum'

            # Column values are interpreted as strings
            elif self.data[self.column].dtype == 'object':
                # If first value is a number, or a comma separated list of numbers, assume it is point data
                if re.match('^[0-9,. ]+$', self.data[self.column][0]):
                    self.data_type = 'point'
                else:
                    # The column consists of strings
                    # Assume it's categorical data, if a "Minutes" column exists in the dataframe
                    if "Minutes" in self.data.columns:
                        self.data_type = 'category'
                    else:
                        self.data_type = 'point'

            else:
                raise ValueError('Unimplemented column type')

    ##########################################
    # Analysis functions

    def filter_data(self, dataset=None, begin_dt=None, end_dt=None):
        if type(dataset) is None:
            dataset = self.data

        # Returns a subset of dataset to match given criteria
        filtered_data = dataset

        # Time Filter
        if begin_dt is not None or end_dt is not None:
            begin_dt = dataset.index[0] if begin_dt is None else begin_dt
            end_dt = dataset.index[-1] if end_dt is None else end_dt

            # If dates are given, convert them to datetimes
            if type(begin_dt) == dt.date:
                begin_dt = midnight(begin_dt)

            if type(end_dt) == dt.date:
                end_dt = eod(end_dt)

            # Get the total entries within this time window
            filtered_data = self.data.loc[(self.data.index >= begin_dt) & (self.data.index <= end_dt)]

        #       if column != None:
        # Column should be a 3-tuple (column_name,

        return filtered_data

    def count_entries(self, *args, **kwargs):
        # Counts the total number of data points between two datetimes
        fdata = self.filter_data(self.data, *args, **kwargs)
        return len(fdata)

    def get_timings(self, date):
        if date not in self.timings.keys() or self.timings[date] is None:
            self.calculate_day(date)

        return self.timings[date]

    def get_total(self, date):
        if date not in self.totals:
            self.calculate_day(date)

        return self.totals[date]

    def total_in_range(self, begin_dt, end_dt):
        # Sums up the totals across a range of dates.
        # Consistent with other ranges: extends to, but does not include end_dt

        # begin_ and end_dt can be a date, in which case just use midnight of that date
        if type(begin_dt) == dt.date:
            begin_date = begin_dt
            begin_dt = dt.datetime.combine(begin_date, dt.time.min())
        else:
            begin_date = begin_dt.date()

        if type(end_dt) == dt.date:
            end_date = end_dt
            end_dt = dt.datetime.combine(end_date, dt.time.min())
        else:
            end_date = end_dt.date()


        ## Process the total for the begin and end dates explicitly since they may have partial days

        # begin_date




###################################################

    def calculate_day(self, date):
        """
        Data type independent function to get timings and totals for the day
        """

        # This class uses memory to avoid repetitious look ups of data
        # If data has already been calculated, don't recalculate
        if date != dt.date.today() and date in self.timings:
            return

        if self.data_type == 'block':
            day_timing = self.day_timing_blockdata(date)
            day_total = self.timings_to_minutes(day_timing)

        elif self.data_type == 'category':
            day_timing = self.day_timing_catdata(date)
            day_total = self.timings_to_minutes(day_timing)

        elif self.data_type == 'point':
            date_str = date.strftime("%Y-%m-%d")
            try:
                day_timing = self.data[date_str].index
                day_total = len(day_timing)
            except KeyError:
                day_timing = []
                day_total = 0

        elif self.data_type == 'sum':
            date_str = date.strftime("%Y-%m-%d")
            try:
                day_timing = self.data[date_str].index
                # Fill in blanks with a default value if specified in the header
                if self.column in self.defaults.keys():
                    day_total = self.data[date_str].fillna(self.defaults[self.column]).sum()[self.column]
                else:
                    day_total = self.data[date_str].sum()[self.column]

            except KeyError:
                day_timing = []
                day_total = 0

        else:
            raise RuntimeError('Data type is invalid or not supported yet')

        # Store the calculations for easy access later
        self.timings[date] = day_timing
        self.totals[date] = day_total


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
                if not in_block:
                    # Special case: if end occurs as the first entry in the day, and the previous day is unknown
                    # Start a virtual block at midnight
                    if in_block is None:
                        block_start = timestamp.floor('D')
                        start_times += [block_start]
                    else:
                        raise ValueError('end cannot occur after another end: ' + str(timestamp) + str(entry))

                in_block = False
                block_end = timestamp

                # Add this to the block dict
                end_times += [block_end]

                continue

            elif pd.isna(entry) or entry == 'nan':
                # No time data for this entry (but other data might be available)
                # Just record the start and end times as being the same
                # TODO: Maybe change how this is represented in the data
                start_times += [timestamp]
                end_times += [timestamp]

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
                block_end = dt.datetime.combine(date, dt.time.max)

            end_times += [block_end]

        # Each block should have a start and an end, therefore start_times and end_times should be the same length
        if len(start_times) != len(end_times):
            raise RuntimeError('start and end arrays don\'t have the same length')

        # Zip the two lists together into a list of tuples, (start_time, end_time)
        return list(zip(start_times, end_times))

    def day_timing_catdata(self, date):
        """
        Gets the start and end times for each category block on a given date
        :param date:
        :return cat_day_blocks { cat -> [list of (start_time, end_time)] }:
        """

        # For some reason, pandas needs time based indexing of time series to be a string
        date_str = date.strftime("%Y-%m-%d")


        # Load the data for the given date
        try:
            df = self.data[date_str]
        except KeyError:
            df = pd.DataFrame(columns=self.data.columns)

        # Initialize empty dicts to store the start times and end times for each category
        start_times = {}
        end_times = {}

        # in_block tracks whether or not the data parser is inside of a start...end block
        # Use the previous entries in the data to determine how this day should start
        start_value = self.day_start_val(date)
        in_block = start_value

        # If starting in a block, add midnight to the start list
        if start_value:
            block_start = dt.datetime.combine(date, dt.time())
            start_times.setdefault(start_value, []).append(block_start)

        # A new data entry into categorical data should always change something (even if in another column)
        # Therefore, if two chronologically consecutive, non-temporary entries are doing the same thing, throw an error
        perms = df[pd.isna(df['Minutes'])].drop(['Minutes'], axis=1)
        conflicts = (perms == perms.shift()).all(axis=1)
        if conflicts.any():
            raise ValueError("Redundant category data: {} {}".format(self.name, conflicts[conflicts].index[0]))

        # Iterate through entries on day
        for timestamp, col, minutes in df[[self.column, "Minutes"]].itertuples():

            # col can either start or change to a block of a new category or if 'end' ends the active category
            # For category data, entries may be made temporary by using numbers in the Minutes column

            if col == 'start':
                raise ValueError('start should not occur in category file')

            elif col == 'end':

                # If nothing is in the minutes column, end the current block (if one is active)
                if pd.isna(minutes):
                    if in_block:
                        end_times.setdefault(in_block, []).append(timestamp)
                        in_block = False

                    continue

                # If minutes is a number, indicates only a temporary end, or a 'whole'
                elif is_number(minutes):
                    # Needs to be a whole in an active block
                    # TODO ensure that block is big enough to subtract
                    if not in_block:
                        continue
                        # raise ValueError("Whole must occur within a block: {} {}".format(self.name, timestamp))

                    # Make the whole
                    whole_start = timestamp - dt.timedelta(minutes=abs(minutes))
                    whole_end = timestamp

                    end_times.setdefault(in_block, []).append(whole_start)
                    start_times.setdefault(in_block, []).append(whole_end)
                    continue

                else:
                    # Unexpected value in the Minutes column
                    raise ValueError("Unexpected value in minutes column: {} {}".format(self.name, timestamp))

            elif col == '[end]' or col == '[start]':
                # These are commands relating to starting and stopping data collection as a whole
                # For now, just skip
                continue

            if is_number(col):
                # Using a negative number in the category column is no longer supported
                # Instead, use 'end' and a positive number in the Minutes column
                raise ValueError('Number should not be in a category column: {} {}'.format(self.name, timestamp))

            else:
                # Otherwise, treat the entry as a category name

                # Behavior in this instance is dependant on minutes
                # If minutes is NaN, start a new category
                if pd.isna(minutes):
                    # End the current category
                    if in_block:
                        end_times.setdefault(in_block, []).append(timestamp)

                    # Start the new category
                    in_block = col
                    start_times.setdefault(col, []).append(timestamp)

                elif minutes >= 0:
                    # Create a block of size minutes

                    mini_block_start = timestamp - dt.timedelta(minutes=minutes)
                    mini_block_end = timestamp

                    # If another block is currently active, stop it for the duration of this new category
                    if in_block:
                        end_times.setdefault(in_block, []).append(mini_block_start)
                        start_times.setdefault(in_block, []).append(mini_block_end)

                    start_times.setdefault(col, []).append(mini_block_start)
                    end_times.setdefault(col, []).append(mini_block_end)

                elif minutes < 0:
                    # Special case allowed where the category is listed with a negative number after it
                    # The project MUST match the currently active project though
                    if col == in_block:
                        # Create a num sized whole in the active block
                        whole_start = timestamp - dt.timedelta(minutes=abs(minutes))
                        whole_end = timestamp

                        end_times.setdefault(in_block, []).append(whole_start)
                        start_times.setdefault(in_block, []).append(whole_end)

                    else:
                        raise ValueError(
                            "Negative number used on non active category: {} {}".format(self.name, timestamp))

                else:
                    raise ValueError("Improperly formatted category data: {} {}".format(self.name, timestamp))

        # Being within a block at the end of a day means count to end of day (a microsecond before midnight)
        # One exception to this is if the date is today, in which case the block extends to right now.
        if in_block:
            if date == pd.Timestamp.now().date():
                block_end = dt.datetime.now()
            else:
                block_end = dt.datetime.combine(date, dt.time.max)

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

    def timings_to_minutes(self, timings):
        if type(timings) == list:
            total_minutes = 0

            # Each block looks like (start, end), so subtract to find total length of each block
            for block in timings:
                total_minutes += (block[1] - block[0]).seconds / 60.0

        elif type(timings) == dict:
            # Timings of mulitple categories
            total_minutes = {}

            # day_timing is a hash with each category as its keys, each value is standard looking block data
            for cat in timings:
                total_minutes[cat] = self.timings_to_minutes(timings[cat])

        else:
            raise RuntimeError('Unexpected timings received')

        return total_minutes






##############################################

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

        # Find the index of the entry most recent before the given date
        try:
            prev_ind = self.data.index.get_loc(midnight(date) - dt.timedelta(0, 0, 1), method='ffill')
        except KeyError:
            # If no data exists before the day
            return False

        if self.data_type == 'block':
            while prev_ind >= 0:
                val = self.data.iloc[prev_ind]['Minutes']

                if val == 'start':
                    # In a generic block
                    return True

                if val == 'end':
                    # Not in any block
                    return False

                prev_ind -= 1

        elif self.data_type == 'category':
            while prev_ind >= 0:
                catcol = self.data[[self.column]].iloc[prev_ind][0]
                minutes = self.data[["Minutes"]].iloc[prev_ind][0]

                # Day does not start in_block if category ended the previous day
                if pd.isna(minutes):
                    return False if catcol in ['end', '[start]', '[end]'] else catcol

                # If last entry on the previous day was a temporary entry, continue to previous entry and so on
                prev_ind -= 1

        # If data is of another type, then assume not in a block
        return False


if __name__ == '__main__':
    testact = Datumdata('testact')
    testact.calculate_day(dt.date.today())
