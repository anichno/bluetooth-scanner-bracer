# Overview
Device will continually scan for nearby bluetooth devices. On discovery of a device, first it will check if it has been seen before and then retrieve the saved color for it. If it has not been seen before, a random color will be generated for it and will be saved.

For each visible device, a signal strength bar will be shown on the led strip. Position of the bar is:
 - random, but is "sticky" once assigned (until that device is no longer visible/too weak to show, it will remain in roughly the same position on the bracer)
 - based on signal strength (and can continually update)
Signal strength bars will be vertically expanded to use up "most" of the available lights. Position preference is based on switch input.

If the device sees a "paired" device, it will use its saved color, but position its signal strength bar at a known fixed location. It will then ping that device at an increased rate to improve its update resolution.

If scan resolution is slow, device will attempt to interpolate results such that the bars always expand and reposition in a smooth fashion. All updates will use a "fade" transition.