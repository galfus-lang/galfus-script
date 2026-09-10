use super::http_response_body;

#[test]
fn reads_a_content_length_response_body() {
    let response = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello";

    assert_eq!(http_response_body(response).unwrap(), b"hello");
}

#[test]
fn decodes_a_chunked_response_body() {
    let response = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n";

    assert_eq!(http_response_body(response).unwrap(), b"hello world");
}
