#!/usr/bin/python3

# create-token uses analysis of datum files to figure out the current frequency of a certain set of data.
# A taper is then created to change that frequency to a target frequency by some target date.
# Then, it constructs time-series of 'tokens' in its own datum file.
# This file can then be read by view-token, which will show the difference between reality and the token system.


import pandas as pd
import argparse
import subprocess
import os
import sys
import math
from math import log2

from dateutil import parser as dtparse

from datumdata import Datumdata

import datetime as dt

from miscfunc import *

parser = argparse.ArgumentParser(description='create a token based taper for a datum file')
parser.add_argument('dname', type=str, help='name of the datum file to taper')
parser.add_argument('tfreq', type=float, help='the target value per PERIOD (set with -T, default:30days)')
parser.add_argument('period', type=parse_duration, help='the period corresponding to the target-freq', default=dt.timedelta(30))
parser.add_argument('tdate', type=parse_date, help='the date at which the target_freq should be reached, ' +
                                                                          'can also be a duration (eg "30 days"), for relative goals')

parser.add_argument('--sustain', type=str, help='how long to maintain target-freq after target-date is reached (can be dur or absolute time)', default="30days")
parser.add_argument('--sdate', type=parse_date, help='start date. Starting date of the taper', default=now)
parser.add_argument('--sfreq', dest='sfreq', type=float, help='Starting frequency. Default calculates from --window and the datum file')
# NOT IMPLEMENTED parser.add_argument('--column', type=str, help='column in the datum file to taper. The default column is based upon datatype')
parser.add_argument('--datatype', type=str, help='Datum data type. Defaults to "point"', default="point")
parser.add_argument('--clobber', action='store_true', help='Overwrite existing taper file without prompt')
parser.add_argument('--window', type=parse_duration, help='The time window of the recent past for which to calculate sfreq', default=dt.timedelta(14))
parser.add_argument('--taper', choices=['linear', 'log'], help='The equation used to taper from sFreq to tFreq', default='linear')

# At max, one of --iti and --value can be specified, because locking one to a constant implies letting the other vary.
pgroup1 = parser.add_mutually_exclusive_group()
pgroup1.add_argument(
    '--iti','-i',
    type=parse_duration,
    help=('The inter-token interval (ITI), which is the difference between subsequent entries in the taper datum file. '
         'ITI varies by default, holding --value constant')
)
pgroup1.add_argument(
    '--value','-V',
    type=float,
    help=('The amount added with each token. Units are dependant on the datum file. The units for coffee, for example, would be cups. '
         'By default, this is 1 unless --iti is specified. In this case, value varies.')
)


args = parser.parse_args()

dname = args.dname

period = args.period
period_str = human_dur_str(period)

# Starting values
sdate = args.sdate
sfreq = args.sfreq

# Values at end of taper
tdate = args.tdate
tfreq = args.tfreq

# When to end sustain period
# sustain must be transformed into the proper date here, rather than during parsing, because it relies on tdate
end_date = parse_date(args.sustain, tdate)


if args.iti == None and args.value == None:
    value = 1

# Taper types
# Scale the inter-time-interval (ITI) based on the proportion (p) of the way through the taper timeframe
# Different strategies are available:
# 'linear' - ITI is linearly correlated with p, so in the middle of the timeframe, the ITI will be half way between it's two extremes
#            this leads to an exponential expansion of timings, since longer ITI's increase time between subsequent
# 'log'    - ITI follows a log pattern:
taper = args.taper
if taper == 'linear':
    taper_eq = lambda p : p
elif taper == 'log':
    taper_eq = lambda p : log2(p+1)
else:
    raise ValueError('No such taper: ' + args.taper)

##############################
# Load datum files

datum_dir = '/home/vividn/Data/datum/'
# Check to make sure that there exists a datum file with this name
datum_file = datum_dir + dname + '.tsv'
if not os.path.isfile(datum_file):
    raise ValueError(dname + ' is not a valid datum file')

# taper_file is the output of this function, essentially a mock datum file that maps
# what the base datum file should look like in order to follow the taper.
taper_file = datum_dir + 'taper-' + dname + '.tsv'

# Ask for confirmation before clobbering existing taper file
if os.path.isfile(taper_file) and not args.clobber:
    prompt = "Taper currently active for " + dname + ". Overwrite? [y/N]:"
    yn = input(prompt)
    if len(yn) == 0 or not yn.lower()[0] == 'y':
        exit(2)

# Load the data
data = Datumdata(dname, type=args.datatype)

#######################
# Determine what the current frequency is from collected data
window_begin = sdate - args.window

count = data.count_entries(window_begin, sdate)

# Calculate the starting frequency per period
sfreq = sfreq or count / args.window.total_seconds() * period.total_seconds()

####################
# Build the taper

def get_out_str(entry_time=None, value=None, mode=None, freq=None, period_str=None, until=None, tfreq=None, iti=None):
    # With no arguments, this builds the header string
    if entry_time == None:
        return "Date\tTime\tValue\tITI(days)\tMode\tFreq\tPer\tUntil\tTfreq\n"

    # Otherwise, must include all the following elements in kwargs
    # entry_time, value, mode, freq, period_str, tfreq, until, iti

    date_str = entry_time.strftime("%Y-%m-%d")
    time_str = entry_time.strftime("%H:%M:%S.%f")[0:-3]
    value_str = "{:.4g}".format(value)
    mode_str = mode
    freq_str = "{:.2f}".format(freq)
    tfreq_str = "{:.2f}".format(tfreq)
    until_str = until.strftime("%Y-%m-%d")
    iti_str = "{:.4g}".format(iti.total_seconds() / 60.0 / 60.0 / 24.0)

    return "\t".join([date_str, time_str, value_str, iti_str, mode_str, freq_str, period_str, until_str, tfreq_str]) + "\n"


# Length in seconds of the tapering timeframe
taper_timeframe = tdate - sdate

with open(taper_file, 'w') as f:
    # Header line
    print(get_out_str(), end='')
    f.write(get_out_str())

    # Create the taper, shifting gradually from sfreq to tfreq from sdate until the target_date
    next_entry = sdate
    iti = dt.timedelta(0)
    cur_freq = sfreq

    mode='[start]'

    while next_entry <= end_date:
        out_str = get_out_str(entry_time=next_entry, value=value, mode=mode, freq=cur_freq, period_str=period_str, tfreq=tfreq, until=tdate, iti=iti)
        print(out_str, end='')
        f.write(out_str)

        if next_entry <= tdate:
            # Before target date is reached, taper frequency from sfreq to tfreq
            if tfreq > sfreq:
                mode = taper + '-up'
            else:
                mode = taper + '-down'

            # Taper using the strategy chosen, info above near parser.
            p = (next_entry - sdate) / taper_timeframe
            cur_freq = sfreq + (taper_eq(p) * (tfreq - sfreq))

            # Use cur_freq and period to determine the correct ITI
            iti = period / cur_freq
            next_entry += iti

        else:
            # After the target date, just maintain tfreq for the sustain period
            mode = 'sustain'
            cur_freq = tfreq
            iti = period / tfreq
            next_entry += iti

    # Write the end date as the last entry of the taper file
    mode = '[end]'

    out_str = get_out_str(entry_time=next_entry, value=value, mode=mode, freq=cur_freq, period_str=period_str, tfreq=tfreq, until=tdate, iti=iti)
    print(out_str, end='')
    f.write(out_str)
