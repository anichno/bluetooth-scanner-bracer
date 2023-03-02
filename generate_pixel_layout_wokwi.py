#!/usr/bin/env python3

"""Quick and dirty script to generate the pixel layout for the wokwi simulator."""

vert_step = [5, 7, 14, 25]
horiz_step = 40


def generate_switchback(start: int, rgb_start: int, tot_lights: int):
    flip = False
    lights = 1
    prev_x = 0
    prev_y = start
    rgb = rgb_start

    while True:
        if flip:
            modifier = -1
        else:
            modifier = 1

        flip = not flip

        # gen left side
        for i in range(4):
            print(
                f'{{ "type": "wokwi-neopixel", "id": "rgb{rgb}", "top": {prev_y}, "left": {prev_x}, "attrs": {{}}}},')
            prev_x -= horiz_step*modifier
            prev_y += vert_step[i]
            rgb += 1
            if lights == tot_lights:
                return
            lights += 1

        # gen right side
        for i in range(3, -1, -1):
            # print(prev_x, prev_y)
            print(
                f'{{ "type": "wokwi-neopixel", "id": "rgb{rgb}", "top": {prev_y}, "left": {prev_x}, "attrs": {{}}}},')

            prev_x += horiz_step*modifier
            prev_y += vert_step[i]
            rgb += 1
            if lights == tot_lights:
                return
            lights += 1


def generate_connections(tot_lights: int):
    rgb = 1
    while rgb <= tot_lights:
        if rgb < tot_lights:
            print(f'[ "rgb{rgb}:DOUT", "rgb{rgb+1}:DIN", "", [ "" ] ],')
        print(f'[ "rgb{rgb}:VSS", "esp:GND", "", [ "" ] ],')
        print(f'[ "rgb{rgb}:VDD", "esp:3V3", "", [ "" ] ],')

        rgb += 1


# generate_switchback(180, 11, 50)
generate_connections(60)
