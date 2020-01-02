#!/usr/bin/python3

import pandas as pd
import numpy as np
import datetime
import re
import os
import sys
from PIL import Image, ImageDraw, ImageFont

from pathlib import Path
home = str(Path.home()) + '/'

# Custom class for analyzing datum files
from datumdata import Datumdata
from miscfunc import *

global all_datumdatas
all_datumdatas = {}

def set_constants():
    # Constants and globals
    global data_set
    data_set = 'standardSet'

    global quantifier_set
    quantifier_set = 'quants'

    # Default data color
    global default_color
    default_color = (255, 255, 255)

    # Other colors
    global text_color
    text_color = (255, 255, 255)
    global line_color
    line_color = (255, 255, 255)
    global bg_color
    bg_color = (0, 0, 0)

    # Specify the bar height classes, which can be referred to later by name
    # 0 is the top of the bar, 1 is the bottom
    global bar_span
    bar_span = {'full': (0, 1),
                'level1': (0.20, 1),
                'level2': (0.4, 1),
                'level3': (0.6, 1),

                'upper': (0.1, 0.3),
                'mid': (0.4, 0.6),
                'lower': (0.7, 0.9),
                'floor': (0.9, 1.0),
                'ceiling': (0, 0.07),
                }

    # Point properties
    global point_r
    point_r = 4

    # The Now Line
    global now_color
    now_color = (255, 255, 0)
    global now_width
    now_width = 1

    # Error symbol
    global error_bgcolor
    error_bgcolor = (255, 0, 0)
    global error_fgcolor
    error_fgcolor = (0, 0, 0)
    global error_symbol
    error_symbol = '!'

    # Image properties
    # Total size of the whole image
    global xsize
    xsize = 1080
    global ysize
    ysize = 600

    # Font
    global fontpath
    fontpath = home + '.fonts/typecatcher/'

    global fontsize
    fontsize = 18
    global imfontname
    imfontname = 'Open Sans_regular.ttf'

    global quantfontsize
    quantfontsize = 16
    global quantfontname
    quantfontname = 'Open Sans_800.ttf'


    # Padding (in pixels)
    global image_padding
    image_padding = 20  # outer border
    global bar_padding
    bar_padding = 8  # space between bars
    global text_padding
    text_padding = 10  # Space between text and bars
    global quant_padding
    quant_padding = 5

    # If neither begin_date nor end_date is specified, then end_date will be set to today (late in code)
    # If both are specified, then they take precedence over nDays
    global nDays
    nDays = 8
    global begin_date
    begin_date = None
    global end_date
    end_date = None

    # Where to save the image
    global imagedir
    imagedir = home + 'Analysis/Pictures/'
    global imagename
    imagename = 'dayview'


#########################################
#########################################

def main(**kwargs):
    """
    Loads in specified datum files and plots them according to their type to create the dayview graphic
    """
    set_constants()

    # For each of the kwargs, change the specified variable to the new value
    for key, value in kwargs.items():
        globals()[key] = value

    # Set where image is saved
    imageout = imagedir + imagename + '.png'

    # Generate the fonts
    imfont = ImageFont.truetype(fontpath + imfontname, fontsize)
    quantfont = ImageFont.truetype(fontpath + quantfontname, quantfontsize)

    # Display info about what is being processed
    print("\n#####################\n" + imagename)

    # Calculate the begin and end dates
    global begin_date, end_date, nDays
    if begin_date is None:
        if end_date is None:
            # If end_date is not specified, use today
            end_date = dt.date.today()

        begin_date = end_date - dt.timedelta(days=nDays - 1)

    else:
        if end_date is None:
            end_date = begin_date + dt.timedelta(days=nDays - 1)

        else:
            # Overide nDays
            nDays = (end_date - begin_date).days + 1


    # Load in the data to plot from the dayview_table.
    # It includes name, span (vertical bar span), and color of the data
    table_file = os.path.join(os.path.dirname(__file__), data_set + '.csv')
    datum_table = pd.read_csv(table_file, comment="#")

    # Extract the names, spans and colors
    datum_names = []
    datum_spans = {}
    datum_colors = {}
    prev_row = None
    for row in datum_table.itertuples():
        if row.Name != ':':
            datum_names += [row.Name]
            datum_spans[row.Name] = row.Span

            # Don't assign a color if one is not entered
            if not any(np.isnan([row.R, row.G, row.B])):
                datum_colors[row.Name] = (int(row.R), int(row.G), int(row.B))
            else:
                datum_colors[row.Name] = default_color
            prev_row = row.Name

        else:
            # Color categorical data
            # TODO: Allow further nesting with multiple colons

            # If this is the first category, change the color for this data to be a hash, with the proper default
            if datum_colors[prev_row].__class__ != dict:
                datum_colors[prev_row] = {'default': datum_colors[prev_row]}
            # Category name is in the span column
            if not any(np.isnan([row.R, row.G, row.B])):
                datum_colors[prev_row][row.Span] = (int(row.R), int(row.G), int(row.B))
            else:
                datum_colors[prev_row][row.Span] = datum_colors[prev_row]['default']

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

    # Load in the data to quantify (i.e., display the total on the right side of the day)
    # For now, only + is allowed. TODO: Build in other operations for quantification such as AND and OR
    quant_file = os.path.join(os.path.dirname(__file__), quantifier_set)
    quants = []
    with open(quant_file) as f:
        quants = f.read().splitlines()

    # Use 88.8 a maximal width example, to allocate space
    quant_width, quant_height = quantfont.getsize('11.1')
    quant_rows = int(np.floor_divide(day_height, quant_height))
    quant_cols = int(np.ceil(len(quants) / quant_rows))
    total_quant_width = quant_cols * quant_width

    # If there are less quants than rows, reduce the rows to match
    quant_rows = min(quant_rows, len(quants))

    # Bar properties
    l_midnight = image_padding + text_width + text_padding
    r_midnight = xsize - image_padding - total_quant_width - quant_padding
    day_lenx = r_midnight - l_midnight

    #####################################
    # Get the dates and then draw the date text

    # Create an array of dates to analyze
    dates_to_plot = []

    # day0s is an array of the ymin position of each of the days
    day0s = []

    for iDay in range(0, nDays):
        # Starting with the beginning date, add the dates to the array
        date = begin_date + datetime.timedelta(days=iDay)
        dates_to_plot += [date]

        # Record where the day is in the image
        day0 = image_padding + text_height + text_padding + (day_height + bar_padding) * iDay
        day0s += [day0]

        # Create and draw the label
        date_label = date.strftime(date_label_fmt)
        date_width, date_height = draw.multiline_textsize(date_label, font=imfont)
        date_x = l_midnight - text_padding - date_width
        date_y = image_padding + text_height + text_padding + (
                    day_height + bar_padding) * iDay + day_height / 2.0 - date_height / 2.0
        draw.multiline_text(xy=(date_x, date_y), text=date_label, font=imfont, fill=text_color, align='right')

    ################################################
    # Load and iterate through the data
    error_has_occurred = False
    datum_with_error = []
    exception_text = ''

    for name in datum_names:
        print(name)
        span = datum_spans[name]

        # Get the data if it has already been loaded, otherwise use the Datumdata class to load in the data
        if name in all_datumdatas.keys():
            data = all_datumdatas[name]
        else:
            try:
                data = Datumdata(name)
                all_datumdatas[name] = data
            except Exception as e:
                # If an error occurs, continue on, but later draw the error symbol as indication
                error_has_occurred = True
                datum_with_error += [(name, "Load")]
                exception_text += str(e) + "\n"
                continue

        #########################################
        # Plot the data day by day
        for iDay, date in enumerate(dates_to_plot):

            # Load the blocks for the given day
            try:
                data.calculate_day(date)
            except Exception as e:
                # If an error occurs, continue on, but later draw the error symbol as indication
                error_has_occurred = True
                datum_with_error += [(name, date)]
                exception_text += str(e) + "\n"
                continue

            # The upper and lower y values of where to draw the data
            min_block_y = day0s[iDay] + bar_span[span][0] * day_height
            max_block_y = day0s[iDay] + bar_span[span][1] * day_height

            # If data are points, center within the span
            point_y = (min_block_y + max_block_y) / 2

            if data.data_type == 'block':

                # Get the appropriate data color
                data_color = get_color(name, datum_colors)

                # Draw the blocks
                for block_start, block_end in data.timings[date]:
                    if block_start != block_end:

                        # Figure out where the block starts and ends in the day on the image
                        xmin = dayperc(block_start.time()) * day_lenx + l_midnight
                        xmax = dayperc(block_end.time()) * day_lenx + l_midnight

                        draw.rectangle([xmin, min_block_y, xmax, max_block_y], fill=data_color)

                    else:
                        # If block_start and block_end are the same, there is no duration data for this entry,
                        # So, just draw a point
                        point_x = dayperc(block_end.time()) * day_lenx + l_midnight

                        # Get the bounding box of the dot
                        point_xy = [point_x - point_r, point_y - point_r, point_x + point_r, point_y + point_r]

                        draw.ellipse(point_xy, fill=data_color)


            elif data.data_type == 'category':
                # Load the data for the given day
                all_blocks = data.get_timings(date)

                for cat in all_blocks:
                    # Get the color for the category
                    data_color = get_color(name, datum_colors, cat)

                    # Draw the blocks
                    for block_start, block_end in all_blocks[cat]:
                        # Figure out where the block starts and ends in the day on the image
                        xmin = dayperc(block_start.time()) * day_lenx + l_midnight
                        xmax = dayperc(block_end.time()) * day_lenx + l_midnight

                        draw.rectangle([xmin, min_block_y, xmax, max_block_y], fill=data_color)

            else:
                # Get the appropriate data color
                data_color = get_color(name, datum_colors)

                for timestamp in data.timings[date]:
                    point_x = dayperc(timestamp.time()) * day_lenx + l_midnight

                    # Get the bounding box of the dot
                    point_xy = [point_x - point_r, point_y - point_r, point_x + point_r, point_y + point_r]

                    # Draw the dots
                    draw.ellipse(point_xy, fill=data_color)

    ####################################
    # Show the quantifications on the right side
    for iDay, date in enumerate(dates_to_plot):
        # The upper and lower y values of the entire day
        min_day_y = day0s[iDay]
        max_day_y = day0s[iDay] + day_height

        for iQuant, quant in enumerate(quants):
            quant_color = get_quant_color(quant, datum_colors)
            quant_row = np.mod(iQuant, quant_rows)
            quant_col = np.floor_divide(iQuant, quant_rows)

            # The actual quantification
            quant_num = quantify_day(quant, date)
            # Convert to a nice string
            if type(quant_num) == int:
                quant_str = '{:d}'.format(quant_num)
            else:
                quant_str = '{:.1f}'.format(quant_num)

            # To get the x and y, divide the total height of the day into equal sections
            # and then center the number in that section
            quant_x = r_midnight + quant_padding + (quant_width * quant_col)
            quant_y = min_day_y + ((0.5 + quant_row) * day_height / quant_rows) - (quant_height / 2)

            draw.text(xy=(quant_x, quant_y), text=quant_str, font=quantfont, fill=quant_color)

    # All data are now drawn

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
        draw.text(xy=(text_x, text_y_top), text=text_str, font=imfont, fill=text_color)
        draw.text(xy=(text_x, text_y_bottom), text=text_str, font=imfont, fill=text_color)

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
    now_bottom = ysize - image_padding
    now_top = day0s[-1]
    draw.line((now_x, now_top, now_x, now_bottom), fill=now_color, width=now_width)

    # Display error symbol if error has occured
    if error_has_occurred:
        ex = image_padding
        ey = ysize - image_padding - text_height

        # Display the error symbol plus the first 3 characters of the datum name
        error_text = error_symbol + ' ' + datum_with_error[0][0][0:3]

        # To make the error more visible, put it on a bright background color
        esize = draw.textsize(error_text, font=imfont)
        draw.rectangle([ex, ey + 3, ex + esize[0], ey + esize[1] + 3], fill=error_bgcolor)
        draw.text(xy=(ex, ey), text=error_text, font=imfont, fill=error_fgcolor)
        print(exception_text, file=sys.stderr)

    im.save(imageout)


###########################################################
###########################################################

def quantify_day(quant_str, date):
    '''
    Quantifies a given day based on a mathematical string including the various variables
    For now, can only handle addition
    :param quant_str:
    :param date:
    :return: quantity
    '''

    grand_total = 0
    data_names = [x.strip() for x in quant_str.split("+")]

    for component in data_names:
        try:
            if ':' in component:
                name, cat = component.split(':',1)
                if cat in all_datumdatas[name].totals[date].keys():
                    total = all_datumdatas[name].totals[date][cat]
                else:
                    total = 0
            else:
                name = component
                total = all_datumdatas[name].totals[date]
                
            # If these data are in minutes, divide to hours
            if all_datumdatas[name].data_type in ['block', 'category']:
                total = total/ 60.0

        except KeyError:
            total = 0

        grand_total += total

    return grand_total

def get_quant_color(quant_str, datum_colors):
    # Uses the color of the first defined datum name in the quant_str
    name = quant_str.split("+")[0].strip()
    if ':' in name:
        name, cat = name.split(':',1)
    else:
        cat = None

    quant_color = get_color(name, datum_colors, cat)

    # Dark colors can be hard to see with just text, so lighten them
    while np.sum(quant_color) < 255:
        quant_color = tuple(map(lambda x: min(x+1, 255), quant_color))

    return quant_color

def dayperc(time):
    # Returns the percentage through the day that a certain time is
    return time.hour / 24.0 + time.minute / (24.0 * 60) + time.second / (24.0 * 60 * 60)


def get_color(datum_name, datum_colors, category=None):
    if category is None:
        try:
            return datum_colors[datum_name]
        except KeyError:
            return default_color

    else:
        try:
            return datum_colors[datum_name][category]
        except KeyError:
            # If the category has subprojects attached (e.g., category:subcategory), get the color of the super category
            if ':' in category:
                super_category = ':'.join(category.split(':')[0:-1])
                return get_color(datum_name, datum_colors, super_category)

            # Categorical data can have it's own default
            try:
                return datum_colors[datum_name]['default']
            except KeyError:
                return default_color


main()
main(imagename='reduced', data_set='reducedSet')
main(nDays=30, imagename='30dayview', ysize=1920)
main(nDays=1, imagename='singledayview',
     xsize=1920, ysize=120, image_padding=6, bar_padding=0, text_padding=0, quantfontsize=18)
main(nDays=1, imagename='phoneday',
     xsize=1080, ysize=200, image_padding=6, bar_padding=0, text_padding=0, now_width=2,
     quantfontsize=30)
main(begin_date=dt.date(2018,12,9), end_date=dt.date(2018,12,15), imagename='interview', data_set='reducedSet',
     xsize =1920, ysize=1080, fontsize=24, quantfontsize=30, point_r=7)
#main(nDays=1, imagename='sampleAct', data_set='actSample', begin_date=dt.date(2019,1,28),
#     xsize=1920, ysize=120, image_padding=6, bar_padding=0, text_padding=0, quantfontsize=18)
