-- Add migration script here
CREATE TABLE users (
    id serial primary key,
    name varchar(255) not null unique,
    hash varchar not null,
    salt varchar not null,
    createdAt timestamp not null default now (),
    deleted boolean not null default false
);
