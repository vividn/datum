/*
Let's see here.
I want a start command and then a corresponding stop command
Also an occur command
hmmmmm

for block data:
start <field>
end <field>
occur <field> [duration]
/// pause <field> <duration> -- like doing `occur <field> -duration`
if [duration] is "start" or "end", then automatically transform into the respective start or end type data

for block data in the end it should be relatively equivalent to using state data with states of true and false

for point data:
occur <field>

for state data:
switch <field> <state>
Should include information about what the last state was for mapreduce totalling later, must have some error checking function


 */

