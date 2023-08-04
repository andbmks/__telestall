FROM rust:1.71 as build
WORKDIR /prod

COPY . .

RUN cargo build --release

CMD ["/prod/target/release/telestall"]
