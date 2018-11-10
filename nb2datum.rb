#!/usr/bin/ruby
require 'date'

# Process all my notebook files to track the amount that I write

data_dir = '/home/vividn/Notebooks/'
words_file = '/home/vividn/Data/datum/words.tsv'
blocks_file = '/home/vividn/Data/datum/write.tsv'

# Save the datum data in an array first to allow for sorting before writing
words_output = []
blocks_output = []

# Go through each notebook and extract number of words written per day
Dir.glob("#{data_dir}*.md") do |notebook|
  # Get the name of the notebook without the rest of the filename and .md
  name = notebook.split('/')[-1].chomp('.md')
  print("Processing: #{name}\n")

  # Initialize date, time, and word variables
  entryDate,newEntryDate,endDate = nil
  entryTime,newEntryTime,endTime = nil
  entryWordCount = 0;
  
  # Count the number of lines in the file so that on the last line, one final entry can be made
  nLines = %x{wc -l #{notebook}}.split.first.to_i
  iLine = 0

  # Load the notebook file into memory.
  File.open(notebook,'r').each do |line|
    
    # Increment line counter
    iLine += 1
    
    # If a new entry has been started, write the last entry's data to the output
    if newEntryDate != entryDate || newEntryTime != entryTime || iLine == nLines
      # Only write if wordcount is > 0
      if entryWordCount > 0
        # If there is an END_OF_ENTRY, record an appropriate start...end block
        # Record number of words at the end of the block
        if endTime
            blocks_output << sprintf("%s\t%s\t%s\t%s",entryDate,entryTime,"start",name)
            blocks_output << sprintf("%s\t%s\t%s\t%s",endDate,endTime,"end",name)
            
            words_output << sprintf("%s\t%s\t%d\t%s",endDate,endTime,entryWordCount,name)
        else
            # If there is no END_OF_ENTRY, still record the entry, but time will be considered 0
            blocks_output << sprintf("%s\t%s\t%d\t%s",entryDate,entryTime,0,name)
            
            words_output << sprintf("%s\t%s\t%d\t%s",entryDate,entryTime,entryWordCount,name)
        end
      end

      entryDate = newEntryDate
      entryTime = newEntryTime
      endDate, endTime = nil
      entryWordCount = 0
    end


    # There are 2 types of lines that change the date and don't count towards word count

    # "## YY-mm-dd, DOW"
    match = line.match(/^## ?(\d{4}-\d{2}-\d{2}),/)
    next if match

    # "*[HH:MM:SS ~ YYYY-mm-dd]*" May also be "*END OF ENTRY [...]*"
    match = line.match(/^ ?\*(END OF ENTRY )?\[(\d{2}:\d{2}:\d{2}) ~ (\d{4}-\d{2}-\d{2})\]\* *$/)
    if match

      if match[1]
          # If this is an END OF ENTRY record the end time and date
          endTime = match[2] + '.000'
          endDate = match[3]
          next
      
      else
      
        # Otherwise a match indicates a start of a new entry, update entryTime and entryDate
        newEntryTime = match[2] + '.000'
        newEntryDate = match[3]
        next
        
      end
      
    end

    # Skip the lines before the first date entry
    next unless entryDate

    # For the rest of lines count the words and add them to the hash
    wordCount = line.split.size

    # Add the wordcount to the current entry 
    entryWordCount += wordCount
  end

end


# Sort the output to interlace the entries across all the notebooks
words_output.sort!
blocks_output.sort!

# Write all the data to the datum file
File.open(words_file,'w') do |f|

  # Header line
  f.puts("Date\tTime\tWords\tNotebook")

  # Data
  words_output.each { |line| f.puts(line) }
end

File.open(blocks_file, 'w') do |f|
    
    # Header line
    f.puts("Date\tTime\tMinutes\tNotebook")
    
    # Data
    blocks_output.each { |line| f.puts(line) }
end
