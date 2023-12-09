#!/usr/bin/env python3

import os
import sys
import traceback

import pandas as pd
import numpy as np
import datetime as dt
from PIL import Image, ImageDraw, ImageFont
from pathlib import Path

from datumdata import Datumdata
from dayview import get_datum_names_spans_and_colors, get_color, set_constants


def main(tiny=False):
    set_constants()
    home = str(Path.home()) + "/"
    data_set = "standardSet"
    bg_color = (0, 0, 0)

    error_bgcolor = (255, 0, 0)
    error_fgcolor = (0, 0, 0)
    error_symbol = "!"

    image_padding = 10
    bar_span = {
        "full": (0, 1),
        "level1": (0.20, 1),
        "level2": (0.4, 1),
        "level3": (0.6, 1),
        "upper": (0.1, 0.3),
        "mid": (0.4, 0.6),
        "lower": (0.7, 0.9),
        "floor": (0.9, 1.0),
        "ceiling": (0, 0.07),
        "foundation": (0.94, 1.0)
    }

    fontpath = home + ".fonts/typecatcher/"
    fontsize = 18
    imfontname = "Open Sans_regular.ttf"
    imfont = ImageFont.truetype(fontpath + imfontname, fontsize)
    _, text_height = imfont.getsize("Wed, Dec 88")

    xsize = 50 if tiny else 75
    ysize = 80 if tiny else 150
    imagedir = home + "Analysis/Pictures"
    imagename = "nowviewtiny" if tiny else "nowview"
    imageout = imagedir + '/' + imagename + ".png"

    ##########################3
    # Display info about what is being processed
    print("\n#####################\n" + imagename)

    datum_names, datum_spans, datum_colors = get_datum_names_spans_and_colors(data_set=data_set)

    im = Image.new("RGB", size=(xsize, ysize), color=bg_color)
    draw = ImageDraw.Draw(im)

    day_height = ysize - 2 * image_padding
    xmin = image_padding
    xmax = xsize - image_padding

    ################################################
    # Load and iterate through the data
    error_has_occurred = False
    datum_with_error = []

    for name in datum_names:
        print(name)
        span = datum_spans[name]

        # Get the data if it has already been loaded, otherwise use the Datumdata class to load in the data
        try:
            data = Datumdata(name)
        except Exception as e:
            # If an error occurs, continue on, but later draw the error symbol as indication
            error_has_occurred = True
            datum_with_error += [(name, "Load")]
            print(traceback.format_exc(), file=sys.stderr)
            print(e, file=sys.stderr)
            continue

        # Load the blocks for today
        try:
            data.calculate_day(dt.date.today())
        except Exception as e:
            # If an error occurs, continue on, but later draw the error symbol as indication
            error_has_occurred = True
            datum_with_error += [(name, "Calculation")]
            print(traceback.format_exc(), file=sys.stderr)
            print(e, file=sys.stderr)
            continue


        if data.active_now:
            min_block_y = image_padding + bar_span[span][0] * day_height
            max_block_y = image_padding + bar_span[span][1] * day_height

            if data.data_type == "block":
                # Get the appropriate data color
                data_color = get_color(name, datum_colors)
                draw.rectangle(
                    (xmin, min_block_y, xmax, max_block_y), fill=data_color
                )

            elif data.data_type == "category":
                data_color = get_color(name, datum_colors, data.active_now)
                draw.rectangle(
                    (xmin, min_block_y, xmax, max_block_y), fill=data_color
                )

        # Display error symbol if error has occured
        if error_has_occurred:
            ex = image_padding
            ey = ysize - image_padding - text_height

            # Display the error symbol plus the first 3 characters of the datum name
            error_text = error_symbol + " " + datum_with_error[0][0][0:3]

            # To make the error more visible, put it on a bright background color
            esize = draw.textsize(error_text, font=imfont)
            draw.rectangle(
                (ex, ey + 3, ex + esize[0], ey + esize[1] + 3), fill=error_bgcolor
            )
            draw.text(xy=(ex, ey), text=error_text, font=imfont, fill=error_fgcolor)

    im.save(imageout)

if __name__ == "__main__":
    main()
    main(tiny=True)