from typing import Tuple, Sequence, Optional

from PIL import Image
from PIL import ImageDraw

from datumdata import Datumdata
import datetime as dt

from utils import date_range


def get_imdraw(
    win_xsize=1000, win_ysize=1000, bg_color=(0, 0, 0), **kwargs
) -> ImageDraw.Draw:
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
    draw: ImageDraw.Draw,
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
        colors = ()
    if p_radius < 0 or p_radius > 1:
        raise ValueError("p_radius must be between 0 and 1")

    pass


if __name__ == "__main__":
    datum_piechart("activity")
