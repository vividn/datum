from typing import Tuple, Sequence, Optional

from PIL import Image
from PIL import ImageDraw

from datumdata import Datumdata
import datetime as dt

from utils import date_range, random_color


def get_imdraw(
    win_xsize=1000, win_ysize=1000, bg_color=(0, 0, 0), **kwargs
) -> ImageDraw.ImageDraw:
    im = Image.new("RGB", size=(win_xsize, win_ysize), color=bg_color)
    return ImageDraw.Draw(im)


def datum_piechart(datum_name: str, **kwargs):
    draw = get_imdraw(**kwargs)

    dd = Datumdata(datum_name)
    dt.date.today()

    end_date = dt.date.today()
    begin_date = end_date - dt.timedelta(days=14)

    if dd.data_type != "category":
        raise ValueError("Only category data can be charted with a pie chart")

    totals = {}
    for date in date_range(begin_date, end_date, inclusive=True):
        day_totals = dd.get_total(date)

        for cat in day_totals:
            totals[cat] = totals.get(cat, 0) + day_totals[cat]

    print(totals)


def draw_piechart(
    draw: ImageDraw.ImageDraw,
    data_seq: Sequence[float],
    colors: Optional[Sequence[Tuple[int, int, int]]] = None,
    p_radius: float = 1,
):
    if colors:
        if len(colors) != len(data_seq):
            raise ValueError(
                "If colors are specified, they must match the length of the data"
            )
    else:
        colors = (random_color() for _ in range(len(data_seq)))

    if p_radius < 0 or p_radius > 1:
        raise ValueError("p_radius must be between 0 and 1")

    im: Image.Image = draw.im
    xsize, ysize = im.size


def donut_slice(start_angle, end_angle, span, hole_size, color, im_size=1000):
    im = Image.new("RGBA", size=(im_size, im_size), color=(0, 0, 0, 0))
    draw = ImageDraw.Draw(im)
    xy = [(0, 0), (im_size, im_size)]

    draw.pieslice(xy, start=start_angle, end=end_angle, fill=color)


if __name__ == "__main__":
    datum_piechart("activity")
