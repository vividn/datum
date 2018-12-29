#!/usr/bin/python3
import pandas as pd
import argparse
import subprocess
import os
import sys
import math
from math import log2



from datumdata import Datumdata

import datetime as dt
from miscfunc import *


parser = argparse.ArgumentParser(description='create a token based taper for a datum file')
parser.add_argument('dname', metavar='datum', type=str, help='name of the datum file to taper')
parser.add_argument('--date', dest='qdate', type=parse_date, default=dt.datetime.now())
parser.add_argument('--datatype', type=str, help='Datum data type. Defaults to "point"', default="point")
parser.add_argument('--stats', action="store_true", help="Show stats like ITI and freq")
parser.add_argument('--balance', action="store_true" , help='report relative balance compared to taper and exit')

args = parser.parse_args()

dname = args.dname
taper_name = 'taper-' + dname
qdate = args.qdate


real = Datumdata(dname)
taper = Datumdata(taper_name)

rdata = real.data
tdata = taper.data



# When the most recent taper began
sdate = tdata.loc[tdata['Mode'] == '[start]'].index.to_pydatetime()[0]

# The most recent token
recent_token = tdata.loc[tdata.index < qdate].iloc[-1]

# Detect no taper currently active
if recent_token['Mode'] == '[end]':
    exit(2)

# The next token
next_tokens = tdata.loc[tdata.index > qdate].iloc[0:3]
next_token = next_tokens.iloc[0]

# How long until the next token
next_timer =  next_token.name - dt.datetime.now()

# The difference between real and taper data
real_count = real.count_entries(sdate,qdate)
taper_count = taper.count_entries(sdate,qdate)
print(real_count - taper_count, end='')
if not args.balance:
    # Print a timer until the next token
    print(' ~ next in ', end='')
    print(next_timer)

    # Print timings of next 3 tokens
    for t in next_tokens.index.values:
        t = pd.to_datetime(str(t))
        print(t.strftime('%Y-%m-%d %H:%M:%S'))

if args.stats:
    # Print stats
    print('-------')
    print(next_token[['Iti', 'Freq', 'Per']])

