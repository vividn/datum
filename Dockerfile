FROM python:3.8-slim

WORKDIR /datum
COPY Pipfile Pipfile.lock /datum/

RUN pip install pipenv

RUN pipenv install --system --deploy --ignore-pipfile

RUN useradd -rms /bin/bash -g 100 -u 1000 user
USER user

ENV PATH="/datum:${PATH}"




