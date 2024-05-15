FROM python:3.11-slim

WORKDIR /datum
COPY pyproject.toml poetry.lock /datum/

RUN pip install poetry

RUN poetry install

RUN useradd -rms /bin/bash -g 100 -u 1000 user
USER user

ENV PATH="/datum:${PATH}"




