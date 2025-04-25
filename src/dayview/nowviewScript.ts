export const getNowviewScript = () => `
  const urlParams = new URLSearchParams(window.location.search);
  let timeShiftMinutes = 0;
  console.log("Client-side timeShiftMinutes from URL:", timeShiftMinutes);

  function animateTimeshift(targetTimeshift) {
    const svg = document.querySelector('svg');
    const dataArea = svg.querySelector('.dataArea');

    // Calculate pixel shift based on time difference
    const timeShiftDiff = targetTimeshift - timeShiftMinutes;
    const minuteWidth = dataArea.getBoundingClientRect().width / 15; // 15 minutes total width
    const pixelShift = timeShiftDiff * minuteWidth / 5; // 5 minutes per shift

    // Only move the data area, leaving current time indicators in place
    dataArea.style.transform = \`translateX(\${pixelShift}px)\`;

    // Update time labels
    updateTimeAxis(targetTimeshift);
  }

  function updateTimeAxis(newTimeshift) {
    const now = new Date();
    now.setMinutes(now.getMinutes() - newTimeshift);

    const timeLabels = document.querySelectorAll('.tick text');
    timeLabels.forEach((label, i) => {
      const minutes = i * 5;
      const time = new Date(now.getTime() - minutes * 60000);
      label.textContent = minutes === 0 ?
        \`\${time.getHours()}:\${time.getMinutes().toString().padStart(2, '0')}\` :
        \`-\${minutes}m\`;
    });
  }

  document.addEventListener('keydown', function(event) {
    if (event.key === 'q') {
      console.log("timeShiftMinutes:", timeShiftMinutes);
      timeShiftMinutes += 5;
      console.log("timeShiftMinutes after:", timeShiftMinutes);
      const targetTimeshift = timeShiftMinutes;
      animateTimeshift(targetTimeshift);
      timeShiftMinutes = targetTimeshift;
    } else if (event.key === 'w' && timeShiftMinutes > 0) {
     timeShiftMinutes -= 5;
      const targetTimeshift = timeShiftMinutes;
      animateTimeshift(targetTimeshift);
      timeShiftMinutes = targetTimeshift;
    }
  });

  console.log('Enhanced keyboard navigation initialized with timeshift: ' + timeShiftMinutes);
`;