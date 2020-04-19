#!/usr/bin/env python3
import pandas as pd
import datetime as dt
import numpy as np
from datumdata import Datumdata
import math


for datum_name in ['hw', 'work']:
    # Load in the data
    old_act = Datumdata(datum_name).data
    old_proj = Datumdata(datum_name + 'proj').data

    # Convert hw and work block type data into something resembling activity type data
    equiv = {'start': datum_name, 'end': 'end'}

    # Create Activity column and use start and end values to fill in some of the data. Replace the Minutes column with these entries to NaN
    old_act['Activity']=old_act['Minutes'].map(equiv)
    old_act['Minutes'] = pd.to_numeric(old_act['Minutes'], 'coerce')

    # Sanity check. At this point, the only times that minutes should be NaN, is if they were 'start' or 'end' before.
    # Thus
    assert not any(pd.isna(old_act['Minutes'][pd.isna(old_act['Activity'])]))

    # If minutes is greater than 0, put hw in activity column
    # if < 0, end in the activity column, also make minutes positive at that point
    old_act.loc[old_act.Minutes > 0, 'Activity'] = 'hw'
    old_act.loc[old_act.Minutes <= 0, 'Activity'] = 'end'

    old_act.Minutes = old_act.Minutes.apply(abs)

    # Fill in the Activity column in the project data
    old_proj.loc[old_proj.Project != 'end', 'Activity'] = 'hw'

    # Now interleave the project data
    new_act = pd.concat([old_act, old_proj], sort=True).sort_index()

    # Reorder columns
    new_act = new_act[['Activity', 'Project', 'Minutes']]


    # Try to combine rows with timestamps that are very close together
    close_timestamps = new_act.index.to_series().diff() < dt.timedelta(seconds=1)

    # Every instance of this should occur in isolation (there shouldn't be three close timestamps in a row)
    too_close = close_timestamps & close_timestamps.shift()
    assert len(too_close[too_close]) == 0

    close_idx = np.flatnonzero(close_timestamps)

    # Iterate through the timestamps and compare them to their close companions
    for idx in close_idx:
        rows = new_act.iloc[idx - 1:idx + 1]

        # For each column, the values should match between the entries or at least one should be NaN
        # assert ( (rows.iloc[0] == rows.iloc[1]) | (rows.isna().any()) ).all()

        for col in rows.columns:
            # Combine each column choosing the non-NaN value if it exists, and replace the entry that came first
            new_act.ix[idx - 1, col] = rows.iloc[1][col] if pd.isna(rows.iloc[0][col]) else rows.iloc[0][col]

    # Now remove the the second entry which has been combined into the first
    new_act.iloc[close_idx] = np.nan
    new_act.dropna(axis=0, thresh=1, inplace=True)

    # The remaining cases where Activity is NaN, should be when the project was ended, but hw was not
    new_act.loc[new_act['Activity'].isna(), 'Activity'] = 'hw'
    new_act.loc[new_act['Project'].isna(), 'Project'] = 'end'

    # Now put this into an output file
    outfile='/home/vividn/Data/datum/{}act.tsv'.format(datum_name)
    new_act.to_csv(outfile, sep="\t")