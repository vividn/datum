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

# The surrounding tokens
recent_token = tdata.loc[tdata.index < qdate].iloc[-1]
next_token = tdata.loc[tdata.index > qdate].iloc[0]

# The difference between real and taper data
real_count = real.count_entries(sdate,qdate)
taper_count = taper.count_entries(sdate,qdate)
print(real_count - taper_count)
