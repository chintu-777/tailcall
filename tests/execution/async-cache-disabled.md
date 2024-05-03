# Async Cache Disabled

```graphql @server
schema @server(port: 8000, queryValidation: false) @upstream(baseURL: "http://jsonplaceholder.typicode.com") {
  query: Query
}

type Post {
  body: String
  id: Int
  title: String
  user: User @http(path: "/users/{{.value.userId}}")
  userId: Int!
}

type Query {
  posts: Post @http(path: "/post?id=1")
}

type User {
  id: Int
  name: String
}
```

```yml @mock
- request:
    method: GET
    url: http://jsonplaceholder.typicode.com/post?id=1
  response:
    status: 200
    body:
      id: 1
      userId: 1
- request:
    method: GET
    url: http://jsonplaceholder.typicode.com/users/1
  expectedHits: 1
  response:
    status: 200
    body:
      id: 1
      name: Leanne Graham
```

```yml @test
- method: POST
  url: http://localhost:8080/graphql
  body:
    query: query { posts { user { name } } }
```