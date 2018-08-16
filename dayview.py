#!/usr/bin/python3

import pandas as pd
import datetime
import re
import os
from PIL import Image, ImageDraw, ImageFont

# Custom class for analyzing datum files
from datumdata import Datumdata


def main():
    """
    Loads in specified datum files and plots them according to their type to create the dayview graphic
    """

    # Load in the data to plot from the dayview_table.
    # It includes name, type, and span (vertical bar span) of the data
    table_file = os.path.join(os.path.dirname(__file__), 'dayview_table.csv')
    all_datum_files = pd.read_csv(table_file)

    # The colors with which to draw the data
    data_colors = {'environment': {'outside': (139, 195, 74), 'inside': (117, 117, 117), 'home': (69, 90, 100)}, 'sleep': (10, 80, 160), 'hw': (255, 0, 0), 'coffee': (100, 50, 0), 'default': (200, 200, 128)}

    # Other colors
    text_color = (255,255,255)
    line_color = (255,255,255)
    bg_color = (0,0,0)

    # Specify the bar height classes, which can be referred to later by name
    bar_span = {'full': (0,1),
                'level1': (0.15, 1),
                'level2': (0.3, 1),
                'level3': (0.45, 1),

                'upper': (0.1, 0.3),
                'mid' : (0.4, 0.6),
                'lower': (0.7, 0.9)
                }



    # The Now Line
    now_color = (255,255,0)
    now_width = 1

    # Create the output image
    # Image properties
    # Total size of the whole image
    xsize = 1080
    ysize = 600

    im = Image.new("RGB", size=(xsize, ysize), color=bg_color)
    draw = ImageDraw.Draw(im)
    imageout = '/home/vividn/Analysis/Pictures/dayview.png'

    # Padding (in pixels)
    image_padding = 20  # outer border
    bar_padding = 12  # space between bars
    text_padding = 10  # Space between text and bars

    # Font
    fontsize = 18
    imfont = ImageFont.truetype('/home/vividn/.fonts/typecatcher/Open Sans_regular.ttf', fontsize)
    # Find the width of the date text on the left of the date, in the format Jan 08, so use M's for maximum size
    text_width, text_height = imfont.getsize('Dec 88')

    # Bar properties
    min_bar_x = image_padding + text_width + text_padding
    max_bar_x = xsize - image_padding
    day_lenx = max_bar_x - min_bar_x

    # Point properties
    point_r = 4

    #####################################
    # Get the dates and then draw the date text
    # For now dates are hardcoded to be the previous week including today
    nDays = 8
    end_date = datetime.date.today()

    # Calculate the bar height now for easy centering of the date text
    bar_height = (ysize - 2 * image_padding - 2 * text_height - 2 * text_padding - (nDays - 1) * bar_padding) / nDays

    # Create an array of dates to analyze
    dates_to_plot = []

    for iDay in range(0,nDays):
        # Starting with the beginning date, add the dates to the array
        date = (datetime.date.today() - datetime.timedelta(days=(nDays-iDay-1)))
        dates_to_plot += [date]

        # Create and draw the label
        date_label = date.strftime("%a\n%b %d")
        date_width, date_height = draw.multiline_textsize(date_label, font=imfont)
        date_x = min_bar_x - text_padding - date_width
        date_y = image_padding + text_height + text_padding + ( bar_height + bar_padding ) * iDay + bar_height / 2.0 - date_height / 2.0
        draw.multiline_text(xy=(date_x, date_y), text=date_label, font=imfont, fill=text_color, align='right')


    ################################################
    # Load and iterate through the data
    for datum_file in all_datum_files.itertuples():
        name = datum_file.Name
        datum_type = datum_file.Type
        span = datum_file.Span

        # Use the Datumdata class to load in this data
        data = Datumdata(name, datum_type)

        #########################################
        # Plot the data day by day
        iDay = 0
        for date in dates_to_plot:

            # Load the blocks for the given day
            data.get_day(date)

            min_bar_y = image_padding + text_height + text_padding + ( bar_height + bar_padding ) * iDay
            max_bar_y = min_bar_y + bar_height

            # Get the appropriate data color
            data_color = data_colors[block_file]

            # Draw the blocks
            for block_start, block_end in zip(all_blocks["start"],all_blocks["end"]):
                # Figure out where the block starts and ends in the day on the image
                xmin = dayperc(block_start.time()) * day_lenx  +  min_bar_x
                xmax = dayperc(block_end.time()) * day_lenx  +  min_bar_x

                draw.rectangle([xmin, min_bar_y, xmax, max_bar_y], fill = data_color)

            # Increase iDay by 1 for the next iteration
            iDay += 1


    # Next draw all the single timestamp data as colored points
    for point_file in point_files:
        print(point_file)

        datum_file = datum_dir + point_file + '.tsv'
        data = pd.read_csv(datum_file, delimiter='\t', parse_dates=[[0, 1]], index_col=[0])

        # Convert the date_time column into datetime format
        data.index = pd.to_datetime(data.index)

        # Get the appropriate data color
        data_color = data_colors[point_file]

        #########################################
        # Plot the data day by day
        iDay = 0

        for date in dates_to_plot:
            # For some reason these pandas timeseries and dataframe objects can't take a date object as an index, so convert to a string
            date_str = date.strftime("%Y-%m-%d")

            try:
                 dd = data[date_str]
            except KeyError:
                # This error occurs if the date is after the last actual entry, e.g. today when whatever hasn't happened today.
                # Don't draw anything and just continue.
                continue

            # # If no data exists for the day, continue
            # if len(dd) == 0:
            #     continue

            point_y = image_padding  +  text_height  +  text_padding  +  ( bar_height + bar_padding ) * iDay  +  bar_height / 2

            for timestamp in dd.index:
                point_x = dayperc(timestamp.time()) * day_lenx  +  min_bar_x

                # Get the bounding box of the dot
                point_xy = [point_x - point_r, point_y - point_r, point_x + point_r, point_y + point_r]

                # Draw the dots
                draw.ellipse(point_xy, fill=data_color)

            iDay += 1


    # Draw the times and lines (draw every three hours)
    for i in range(0, 25, 1):
        text_str = str(i)
        text_width = imfont.getsize(text_str)[0]

        # Calculate where to draw the text
        time_x = i / 24.0 * day_lenx + min_bar_x
        text_x = time_x - text_width / 2
        text_y_top = image_padding
        text_y_bottom = ysize - image_padding - text_height

        # Draw the text on the top and bottom of the display
        draw.text(xy=(text_x, text_y_top), text=text_str, font=imfont, color=text_color)
        draw.text(xy=(text_x, text_y_bottom), text=text_str, font=imfont, color=text_color)

        # Draw a line between the top and bottom
        line_top = text_y_top + text_height + 2
        line_bottom = text_y_bottom

        # Make the 0,6,12,18,24 lines a bit bolder
        if (i % 6) == 0:
            line_width = 2
        elif (i % 6) == 3:
            line_width = 0
        else:
            continue

        draw.line((time_x, line_top, time_x, line_bottom), fill=line_color, width=line_width)

    # Draw a Yellow line that represents now
    now_x = dayperc(datetime.datetime.now().time()) * day_lenx + min_bar_x
    now_bottom = ysize - image_padding;
    now_top = now_bottom - text_height - text_padding - bar_height
    draw.line((now_x, now_top, now_x, now_bottom), fill=now_color, width=now_width)

    im.save(imageout)

def get_all_blocks(ts, start_value=None):
    # Takes the timeseries and extracts when blocks start and end, for the purposes of drawing these blocks to the screen

    # Create a dictionary to store the start and end times of each block of time
    # If no entries exist for the day, just this will be returned
    all_blocks = dict(start=[], end=[])


    # in_block tracks whether or not the data parser is inside of a start...end block
    # default is None, because it is uncertain if in block from previous day at midnight
    in_block=start_value

    # If starting in a block, add midnight to the start list
    if start_value:
        block_start = ts.index[0].floor('D')
        all_blocks["start"] += [block_start]

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
            all_blocks["start"] += [block_start]

            continue

        elif entry == 'end':
            # 'end' should never occur outside of a block
            if not (in_block):
                # Special case, if end occurs as the first entry in the day, and the previous day is unknown, start a virtual block at midnight
                if in_block == None:
                    block_start = timestamp.floor('D')
                    all_blocks["start"] += [block_start]
                else:
                    raise ValueError('end cannot occur after another end: ' + str(timestamp) + str(entry))

            in_block = False
            block_end = timestamp

            # Add this to the block dict
            all_blocks["end"] += [block_end]

            continue

        elif entry == '[end]' or entry == '[start]':
            # These are commands relating to starting and stopping data collection as a whole just skip these dates
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
                    raise ValueError("Number not correctly associated with blocks\n{timestamp}, {entry}")

                mini_block_minutes = float(entry)

                # Assuming the number record was made at the end of when it occurred, calculate the beginning
                mini_block_start = timestamp - datetime.timedelta(minutes=abs(mini_block_minutes))
                mini_block_end = timestamp

                # Add these to the appropriate arrays in all_blocks
                if mini_block_minutes >= 0:
                    all_blocks["start"] += [mini_block_start]
                    all_blocks["end"]   += [mini_block_end]
                else:
                    all_blocks["start"] += [mini_block_end]
                    all_blocks["end"] += [mini_block_start]

                continue

            else:
                raise ValueError('Entry not properly formed:' + str(timestamp) + str(' , ') + str(entry))

    # Being within a block at the end of a day means count to end of day (midnight)
    # One exception to this is if the date is today, then the block extends to right now.
    if in_block:
        if timestamp.date() == pd.Timestamp.now().date():
            block_end = timestamp.now()
        else:
            block_end = timestamp.ceil('D') - datetime.timedelta(microseconds=1)

        all_blocks["end"] += [block_end]

    # Each block in all_blocks should have a start and an end now, therefore start and end should be the same length
    # if len(all_blocks["start"]) != len(all_blocks["end"]):
    #     raise RuntimeError('start and end arrays don\'t align')

    return all_blocks

def dayperc(time):
    # Returns the percentage through the day that a certain time is
    return time.hour / 24.0  +  time.minute / (24.0 * 60)  +  time.second / (24.0 * 60 * 60)

def next_day_start(ts):
    """
    Uses the last chronological value in a time series to determine how the next day should start at midnight
    :param ts:
    :return next_day_start_str:
    """

    # Ensure that data is chronological
    ts.sort_index(inplace=True)

    # Next day start is based upon how this day ends, so use the last value recorded
    # If last value is ambiguous, keep iterating backwards
    tail_ind = -1
    while abs(tail_ind) <= len(ts):
        val = ts[tail_ind]

        if val == 'start':
            # In a generic block
            return True

        if val == 'end':
            # Not in any block
            return False

        # If the value is a not a number, then next day starts in a category block.
        if not(re.match('^-?[0-9]', val)):
            return val

        # If value is a number (or some other invalid value), just move back to previous entry
        tail_ind -= 1

    # If no data exists on the day
    return None


main()
