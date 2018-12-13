#!/usr/bin/python3

import pandas as pd
import datetime
import re
import os
from PIL import Image, ImageDraw, ImageFont

# Custom class for analyzing datum files
from datumdata import Datumdata
from miscfunc import *

# Constants and globals
# The colors with which to draw the data
data_colors = {'environment': {'outside': (135, 163, 102), 'inside': (117, 117, 117), 'home': (20, 20, 20)}, 'sleep': (20, 40, 120), 'hw': (200, 50, 50), 'default': (255, 255, 0)}

# Other colors
text_color = (255,255,255)
line_color = (255,255,255)
bg_color = (0,0,0)


# Specify the bar height classes, which can be referred to later by name
# 0 is the top of the bar, 1 is the bottom
bar_span = {'full': (0,1),
            'level1': (0.15, 1),
            'level2': (0.4, 1),
            'level3': (0.6, 1),

            'upper': (0.1, 0.3),
            'mid' : (0.4, 0.6),
            'lower': (0.7, 0.9)
            }

# Point properties
point_r = 4

# The Now Line
now_color = (255, 255, 0)
now_width = 1

# Image properties
# Total size of the whole image
xsize = 1080
ysize = 600

# Font
fontsize = 18
imfont = ImageFont.truetype('/home/vividn/.fonts/typecatcher/Open Sans_regular.ttf', fontsize)


# Padding (in pixels)
image_padding = 20  # outer border
bar_padding = 8  # space between bars
text_padding = 10  # Space between text and bars

# For now dates are hardcoded to be the previous week including today
nDays = 8
end_date = datetime.date.today()

# Where to save the image
imageout = '/home/vividn/Analysis/Pictures/dayview.png'

def main(**kwargs):
    """
    Loads in specified datum files and plots them according to their type to create the dayview graphic
    """

    # For each of the kwargs, change the specified variable to the new value
    for key, value in kwargs.items():
        globals()[key] = value

    # Display info about what is being processed
    print("\n#####################\n" + imageout.split("/")[-1])

    # Load in the data to plot from the dayview_table.
    # It includes name, type, and span (vertical bar span) of the data
    table_file = os.path.join(os.path.dirname(__file__), 'dayview_table.csv')
    all_datum_files = pd.read_csv(table_file)

    im = Image.new("RGB", size=(xsize, ysize), color=bg_color)
    draw = ImageDraw.Draw(im)


    # Find the width of the date text on the left of the date,
    date_label_fmt = "%a, %b %d"
    text_width, text_height = imfont.getsize('Wed, Dec 88')

    # Calculate the height of the full day, to center of the date text
    day_height = (ysize - 2 * image_padding - 2 * text_height - 2 * text_padding - (nDays - 1) * bar_padding) / nDays

    # Switch to 2 line date label if days are big enough
    if day_height >= 2 * text_height:
        date_label_fmt = "%a\n%b %d"
        text_width, text_height = imfont.getsize('Dec 88')

    # Bar properties
    l_midnight = image_padding + text_width + text_padding
    r_midnight = xsize - image_padding
    day_lenx = r_midnight - l_midnight



    #####################################
    # Get the dates and then draw the date text

    # Create an array of dates to analyze
    dates_to_plot = []

    # day0s is an array of the ymin position of each of the days
    day0s = []

    for iDay in range(0,nDays):
        # Starting with the beginning date, add the dates to the array
        date = (datetime.date.today() - datetime.timedelta(days=(nDays-iDay-1)))
        dates_to_plot += [date]

        # Record where the day is in the image
        day0 = image_padding + text_height + text_padding + ( day_height + bar_padding ) * iDay
        day0s += [day0]

        # Create and draw the label
        date_label = date.strftime(date_label_fmt)
        date_width, date_height = draw.multiline_textsize(date_label, font=imfont)
        date_x = l_midnight - text_padding - date_width
        date_y = image_padding + text_height + text_padding + ( day_height + bar_padding ) * iDay + day_height / 2.0 - date_height / 2.0
        draw.multiline_text(xy=(date_x, date_y), text=date_label, font=imfont, fill=text_color, align='right')


    ################################################
    # Load and iterate through the data
    for datum_file in all_datum_files.itertuples():
        name = datum_file.Name
        print(name)
        datum_type = datum_file.Type
        span = datum_file.Span

        # Use the Datumdata class to load in this data
        data = Datumdata(name, datum_type)

        #########################################
        # Plot the data day by day
        iDay = 0
        for date in dates_to_plot:

            # Load the blocks for the given day
            data.calculate_day(date)

            min_block_y = day0s[iDay] + bar_span[span][0] * day_height
            max_block_y = day0s[iDay] + bar_span[span][1] * day_height

            if datum_type == 'block':

                # Get the appropriate data color
                try:
                    data_color = data_colors[name]
                except KeyError:
                    data_color = data_colors['default']

                # Draw the blocks
                for block_start, block_end in data.timings[date]:
                    # Figure out where the block starts and ends in the day on the image
                    xmin = dayperc(block_start.time()) * day_lenx  +  l_midnight
                    xmax = dayperc(block_end.time()) * day_lenx  +  l_midnight

                    draw.rectangle([xmin, min_block_y, xmax, max_block_y], fill = data_color)

            elif datum_type == 'category':
                # Load the data for the given day
                all_blocks = data.get_timings(date)

                for cat in all_blocks:
                    # Get the color for the category
                    try:
                        data_color = data_colors[name][cat]
                    except KeyError:
                        data_color = data_colors['default']

                    # Draw the blocks
                    for block_start, block_end in all_blocks[cat]:
                        # Figure out where the block starts and ends in the day on the image
                        xmin = dayperc(block_start.time()) * day_lenx + l_midnight
                        xmax = dayperc(block_end.time()) * day_lenx + l_midnight

                        draw.rectangle([xmin, min_block_y, xmax, max_block_y], fill=data_color)

            elif datum_type == 'point':
                # Get the appropriate data color
                try:
                    data_color = data_colors[name]
                except KeyError:
                    data_color = data_colors['default']

                # Center the point in the span
                point_y = (min_block_y + max_block_y) / 2

                for timestamp in data.timings[date]:
                    point_x = dayperc(timestamp.time()) * day_lenx + l_midnight

                    # Get the bounding box of the dot
                    point_xy = [point_x - point_r, point_y - point_r, point_x + point_r, point_y + point_r]

                    # Draw the dots
                    draw.ellipse(point_xy, fill=data_color)


            # Increase iDay by 1 for the next iteration
            iDay += 1


    # All data is now drawn
    # Draw the times and lines (draw every three hours)
    for i in range(0, 25, 1):
        text_str = str(i)
        text_width = imfont.getsize(text_str)[0]

        # Calculate where to draw the text
        time_x = i / 24.0 * day_lenx + l_midnight
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
    now_x = dayperc(datetime.datetime.now().time()) * day_lenx + l_midnight
    now_bottom = ysize - image_padding;
    now_top = day0s[-1]
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

main()
main(nDays=30, imageout='/home/vividn/Analysis/Pictures/30dayview.png', ysize=1920)
main(nDays=1, imageout='/home/vividn/Analysis/Pictures/singledayview.png',
     xsize=1920, ysize=120, image_padding=6, bar_padding=0, text_padding=0)
main(nDays=1, imageout='/home/vividn/Analysis/Pictures/phoneday.png',
     xsize=1080, ysize=200, image_padding=6, bar_padding=0, text_padding=0, now_width=2)
