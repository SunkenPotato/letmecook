-- Add migration script here

-- delete the table to start over
drop table recipes;

create table recipes (
    id serial primary key,
    createdat timestamp not null default now(),
    deleted boolean not null default false,
    name varchar(255) not null,
    description text,
    cuisine varchar,
    ingredients json not null,
    steps text[]
);
