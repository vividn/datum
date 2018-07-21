#!/usr/bin/python3

import pandas as pd
import datetime
import re
from PIL import Image, ImageDraw, ImageFont


def main():
    # Where the data are
    datum_dir = '/home/vividn/Data/datum/'

    # Which data to draw
    block_files = ['sleep', 'hw', 'workday', 'berlin']
    point_files = ['coffee']

    data_colors = {'sleep': (0, 80, 160), 'hw': (255, 0, 0), 'coffee': (100, 50, 0)}

    # Other colors
    text_color = (255,255,255)
    line_color = (255,255,255)
    bg_color = (0,0,0)

    # Create the output image
    # Image properties
    # Total size of the whole image
    xsize = 1500
    ysize = 800

    im = Image.new("RGB", size=(xsize, ysize), color=bg_color)
    draw = ImageDraw.Draw(im)
    imageout = '/home/vividn/Analysis/dayview.png'

    # Padding (in pixels)
    image_padding = 20  # outer border
    bar_padding = 15  # space between bars
    text_padding = 10  # Space between text and bars

    # Font
    fontsize = 22
    imfont = ImageFont.truetype('/home/vividn/.fonts/typecatcher/Open Sans_regular.ttf', fontsize)
    text_width, text_height = imfont.getsize('8888-88-88')

    # Bar properties
    min_bar_x = image_padding + text_width + text_padding
    max_bar_x = xsize - image_padding
    day_lenx = max_bar_x - min_bar_x

    # Point properties
    point_r = 10

    #####################################
    # Get the dates and then draw the date text
    # For now dates are hardcoded to be the previous week including today
    nDays = 7
    end_date = datetime.date.today()

    # Calculate the bar height now for easy centering of the date text
    bar_height = (ysize - 2 * image_padding - 2 * text_height - 2 * text_padding - (nDays - 1) * bar_padding) / nDays

    # Create an array of strings for indexing of the datum data by date
    all_date_strs = []

    for iDay in range(0,nDays):
        date_dt = (datetime.date.today() - datetime.timedelta(days=(nDays-iDay-1)))

        date_str = date_dt.strftime("%Y-%m-%d")
        all_date_strs += [date_str]

        # Create and draw the label
        date_label = date_dt.strftime("%A\n%Y-%m-%d")
        date_width, date_height = draw.multiline_textsize(date_label, font=imfont)
        date_x = min_bar_x - text_padding - date_width
        date_y = image_padding + text_height + text_padding + ( bar_height + bar_padding ) * iDay + bar_height / 2.0 - date_height / 2.0
        draw.multiline_text(xy=(date_x, date_y), text=date_label, font=imfont, fill=text_color, align='right')


    ################################################
    # Load and iterate through the data


    # First, draw all the data that represent blocks of time as colored rectangles
    for block_file in block_files:
        # Load in the datum file, combining the date and time columns
        datum_file = datum_dir + block_file + '.tsv'
        data = pd.read_csv(datum_file, delimiter='\t', parse_dates=[[0, 1]], index_col=[0])

        # Convert the date_time column into datetime format
        data.index = pd.to_datetime(data.index)

        ts = data['Minutes']

        # Get the appropriate data color
        data_color = data_colors[block_file]

        #########################################
        # Plot the data day by day
        iDay = 0

        for date_str in all_date_strs:
            try:
                all_blocks = get_all_blocks(ts[date_str])
            except KeyError:
                # If no data exist for a given date, don't draw anything and continue to the next date
                continue

            min_bar_y = image_padding + text_height + text_padding + ( bar_height + bar_padding ) * iDay
            max_bar_y = min_bar_y + bar_height

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
        datum_file = datum_dir + point_file + '.tsv'
        data = pd.read_csv(datum_file, delimiter='\t', parse_dates=[[0, 1]], index_col=[0])

        # Convert the date_time column into datetime format
        data.index = pd.to_datetime(data.index)

        # Get the appropriate data color
        data_color = data_colors[point_file]

        #########################################
        # Plot the data day by day
        iDay = 0

        for date_str in all_date_strs:
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
            line_width = 3
        elif (i % 6) == 3:
            line_width = 0
        else:
            continue

        draw.line((time_x, line_top, time_x, line_bottom), fill=line_color, width=line_width)

    im.save(imageout)

def get_all_blocks(ts):
    # Takes the timeseries and extracts when blocks start and end, for the purposes of drawing these blocks to the screen

    # Create a dictionary to store the start and end times of each block of time
    # If no entries exist for the day, just this will be returned
    all_blocks = dict(start=[], end=[])


    # in_block tracks whether or not the data parser is inside of a start...end block
    # it starts with None, because it is uncertain if in block from previous day at midnight
    in_block=None

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
                # Special case, if end occurs as the first entry in the day, start a virtual block at midnight
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
                    raise ValueError('Number not correctly associated with blocks')

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
    if len(all_blocks["start"]) != len(all_blocks["end"]):
        raise RuntimeError('start and end arrays don\'t align')

    return all_blocks

def dayperc(time):
    # Returns the percentage through the day that a certain time is
    return time.hour / 24.0  +  time.minute / (24.0 * 60)  +  time.second / (24.0 * 60 * 60)


main()