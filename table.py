#!/usr/bin/python3

import pandas as pd
import numpy as np
import matplotlib.pylab as plt
import argparse
from datetime import date, timedelta

# Hard coded variables TODO: Remove these in favor of arguments
datum_dir = '/home/vividn/Data/datum/'

# Get arguments from the command line
parser = argparse.ArgumentParser()
parser.add_argument("datum_file", help="Name of the datum file you want to analyze")
args = parser.parse_args()
name = args.datum_file

# Determine the proper path of the data file
if '.' not in name:
    name = name + '.tsv'
datum_file = datum_dir + name


#############################
## Load the datum file, and parse it into a dataframe

data = pd.read_csv(datum_file, delimiter='\t', parse_dates=[[0,1]], index_col=[0])

# Fill in the NaNs with 1s for now. Will get the default value later
data = data.fillna(1)

# Resample to daily using sum method
data = data.resample('D').sum()

# Days that don't have any activity should be marked as 0
data = data.fillna(0)

################################
## Get the values for each day in the last 4 complete workweeks

# Since we want the last 4 complete workweeks, plus the current partial week, get the current day as a number (Sunday = 0) and add 28.
today = date.today()
dow = int(today.strftime("%w"))
delta_start = dow + 28

start_date = today - timedelta(days=delta_start)

# Iterate through all these days and display the value in a formatted calendar
# Header row
printf()

