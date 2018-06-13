#!/usr/bin/python3

from __future__ import print_function

import pandas as pd
import numpy as np
import matplotlib.pylab as plt
import argparse

# Get widgets

from ipywidgets import interact, interactive, fixed, interact_manual
import ipywidgets as widgets


from matplotlib.pylab import rcParams

# Hard coded variables TODO: Remove these in favor of arguments
datum_dir = '/home/vividn/Data/datum/'

# Get arguments from the command line
parser = argparse.ArgumentParser()
parser.add_argument("datum_file", help="Name of the datum file you want to analyze")
args = parser.parse_args()

# Determine the proper path of the data file
name = args.datum_file
if '.' not in name:
    name = name + '.tsv'

datum_file = datum_dir + name


data = pd.read_csv(datum_file, delimiter='\t', parse_dates=[[0,1]], index_col=[0])

# Fill in the NaNs with 1s for now. Will get the default value later
data = data.fillna(1)

# Resample to daily using sum method
data = data.resample('D').sum()

# Days that don't have any activity should be marked as 0
data = data.fillna(0)

def rollplot(win):
    data.rolling(window = win).sum().plot()

#interact(rollplot, win=7)



