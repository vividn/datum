#!/usr/bin/env python3

# Analysis program for datum files

import pandas as pd
import numpy as np
import matplotlib.pylab as plt
import argparse

# Hard coded variables TODO: Remove these in favor of arguments
datum_dir = '/home/vividn/Data/datum/'

# Get arguments from the command line
parser = argparse.ArgumentParser()
parser.add_argument("datum_file", help="Name of the datum file you want to analyze")
parser.add_argument("-b", "--begin", help="Begin date (default: beginning of data collection)")
parser.add_argument("-e", "--end", help="End date (default: today)")
parser.add_argument("-W", "--week", help="Narrow data to the last week or the week prior to end date")
parser.add_argument("-w", "--workweek", help="Narrow data from the corresponding monday to now or end date")
parser.add_argument()
args = parser.parse_args()

