use tokio::prelude::*;

pub fn forward<P1: AsyncRead + AsyncWrite + Send, P2: AsyncRead + AsyncWrite + Send>(
    p1: P1,
    p2: P2,
) -> impl Future<Error = std::io::Error> {
    let (read1, write1) = p1.split();
    let (read2, write2) = p2.split();

    let c1 = tokio::io::copy(read1, write2);
    let c2 = tokio::io::copy(read2, write1);

    c1.join(c2)
}
