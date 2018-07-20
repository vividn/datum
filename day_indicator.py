#!/usr/bin/python3

import pandas as pd
import datetime
import re
from PIL import Image, ImageDraw, ImageFont


def main():
    # Create the output image
    # Image properties
    # Total size of the whole image
    xsize = 1500
    ysize = 1000

    im = Image.new("RGB", size=(xsize, ysize), color=(255, 255, 255))
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
    bar_color = (255, 0, 0)

    ################################################
    # Load the data
    # Load in the workday file, combining the date and time columns
    workday_file = '/home/vividn/Data/datum/hw.tsv'
    data = pd.read_csv(workday_file, delimiter='\t', parse_dates=[[0, 1]], index_col=[0])

    # Convert the date_time column into datetime format
    data.index = pd.to_datetime(data.index)

    # For now, do the past week TODO: allow specification from commandline
    today = datetime.date.today()
    weekago = today - datetime.timedelta(days=6)
    ts = data['Minutes'][weekago.strftime('%Y-%m-%d'):today.strftime('%Y-%m-%d')]

    data_by_day = ts.groupby(pd.Grouper(freq='D'))
    nDays = len(data_by_day)

    #########################################
    # Plot the data day by day
    iDay = 0

    for dd in data_by_day:
        all_blocks = get_all_blocks(dd[1])

        bar_height = ( ysize - 2*image_padding - 2*text_height - 2*text_padding - (nDays -1)*bar_padding ) / nDays

        min_bar_y = image_padding + text_height + text_padding + ( bar_height + bar_padding ) * iDay
        max_bar_y = min_bar_y + bar_height

        # Draw the date
        date_str = dd[1].index[0].strftime("%A\n%Y-%m-%d")
        date_width, date_height = draw.multiline_textsize(date_str, font=imfont)
        date_x = min_bar_x - text_padding - date_width
        date_y = min_bar_y + bar_height/2.0 - date_height/2.0
        draw.multiline_text( xy = (date_x, date_y),  text = date_str, font=imfont, fill=(0,0,0), align = 'right')

        # Draw the blocks
        for block_start, block_end in zip(all_blocks["start"],all_blocks["end"]):
            # Figure out where the block starts and ends in the day on the image
            xmin = dayperc(block_start.time()) * day_lenx  +  min_bar_x
            xmax = dayperc(block_end.time()) * day_lenx  +  min_bar_x

            draw.rectangle([xmin, min_bar_y, xmax, max_bar_y], fill = bar_color)

        # Increase iDay by 1 for the next iteration
        iDay += 1


    # Draw the times and lines (draw every three hours) #TODO: make this depend on image width
    for i in range(0, 25, 3):
        text_str = str(i)
        text_width = imfont.getsize(text_str)[0]

        # Calculate where to draw the text
        time_x = i / 24.0 * day_lenx + min_bar_x
        text_x = time_x - text_width / 2
        text_y_top = image_padding;
        text_y_bottom = ysize - image_padding - text_height

        # Draw the text on the top and bottom of the display
        draw.text(xy=(text_x, text_y_top), text=text_str, font=imfont, fill=(0, 0, 0))
        draw.text(xy=(text_x, text_y_bottom), text=text_str, font=imfont, fill=(0, 0, 0))

        # Draw a line between the top and bottom
        line_top = text_y_top + text_height + 2
        line_bottom = text_y_bottom

        # Make the 0,6,12,18,24 lines a bit bolder
        if (i % 6) == 0:
            line_width = 3
        else:
            line_width = 0

        draw.line((time_x, line_top, time_x, line_bottom), fill=(0, 0, 0), width=line_width)

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