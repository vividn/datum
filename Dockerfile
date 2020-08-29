FROM node:14.8.0-alpine3.11

# Create app directory
WORKDIR /usr/src/app

# Install app dependencies
COPY package.json ./
COPY yarn.lock ./
RUN yarn install --network-timeout 100000

# COPY files
COPY . .

ENTRYPOINT ["node", "-r", "esm", "/usr/src/app/dist/index.js"]
