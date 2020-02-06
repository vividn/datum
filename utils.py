import datetime as dt


def date_range(start, stop, step=1, inclusive=False):
    day_count = (stop - start).days
    if inclusive:
        day_count += 1

    if step > 0:
        range_args = (0, day_count, step)
    elif step < 0:
        range_args = (day_count - 1, -1, step)
    else:
        raise ValueError("date_range(): step arg must be non-zero")

    for i in range(*range_args):
        yield start + dt.timedelta(days=i)

def random_color() -> Tuple[int, int, int]:
