use mysql::*;
use mysql::prelude::*;
use serde::{Serialize, Deserialize};
use serde_json::{json, Value};

pub fn register_user(
    conn: &mut PooledConn,
    client_addr: &str,
) -> serde_json::Value {
    // Step 1: Insert the new client into the table
    if let Err(e) = conn.exec_drop(
        "INSERT INTO clients(IP_port, is_up) VALUES (:client_addr, :is_up)",
        params! {
            "client_addr" => client_addr,
            "is_up" => true,
        },
    ) {
        // If an error occurs during insert, return a failure response with the error
        return json!({
        "status": "failure"
        });
    }

    // Step 2: Retrieve the last inserted ID (user ID)
    let user_id = conn.last_insert_id();
    return json!({
        "status": "success",
        "user_id": user_id
    });
}


pub fn sign_in_user(
    conn: &mut PooledConn,
    client_id: &u64,
    client_addr: &str,
) -> serde_json::Value {
    // Step 1: Check if the client exists
    let check_query = "SELECT COUNT(*) FROM clients WHERE ID = :client_id";
    let count: u64 = conn.exec_first(check_query, params! {
        "client_id" => client_id,
    }).unwrap_or(Some(0))  // If no result, return Some(0) as the default value
    .unwrap();  // Unwrap the option to get the value

    if count == 0 {
        // If client does not exist, return failure response
        return json!({
            "status": "failure"
        });
    }

    let update_query = "UPDATE clients SET IP_port = :client_addr, is_up = :is_up WHERE ID = :client_id";
    if let Err(e) = conn.exec_drop(update_query, params! {
        "client_addr" => client_addr,
        "is_up" => true,
        "client_id" => client_id,
    }) {
        // If an error occurs during update, return failure response
        return json!({
            "status": "failure"
        });
    }

    // Step 3: Return success if everything goes well
    json!({
        "status": "success"
    })
}


pub fn get_images_up(conn: &mut PooledConn, client_id: &u64) -> serde_json::Value {
    // Step 1: Construct the query to join the clients and resources tables
    let query = "
        SELECT 
            c.ID AS client_id, 
            c.IP_port AS client_addr, 
            GROUP_CONCAT(r.image_id) AS image_ids
        FROM 
            clients c
        JOIN 
            resources r ON c.ID = r.client_id
        WHERE 
            c.is_up = true AND c.ID != :client_id
        GROUP BY 
            c.ID, c.IP_port"; // Group by client_id and client_addr

    // Step 2: Execute the query and collect the results
    match conn.exec_map(
        query,
        params! {
            "client_id" => client_id,  // Pass the client_id as a parameter
        },
        |(client_id, client_addr, image_ids): (u64, String, String)| {
            // Parse the comma-separated image_ids into a JSON array
            let image_id_list: Vec<String> = image_ids
                .split(',')
                .map(|s| s.to_string())  // Convert each image_id to a String
                .collect();

            // Return the JSON structure for each client
            json!({
                "client_id": client_id,
                "client_addr": client_addr,
                "image_ids": image_id_list
            })
        }
    ) {
        Ok(results) => {
            // If the query was successful, return the results as a JSON array
            json!({
                "status": "success",
                "data": results
            })
        }
        Err(e) => {
            // If an error occurs during the query execution, return a failure response
            json!({
                "status": "failure",
                "error": e.to_string()
            })
        }
    }
}


pub fn shutdown_client(conn: &mut PooledConn, client_id: u64) -> serde_json::Value {
    // Step 1: Construct the query to update the client's is_up status to false
    let query = "
        UPDATE clients
        SET is_up = false
        WHERE ID = :client_id";

    // Step 2: Execute the query
    match conn.exec_drop(query, params! {
        "client_id" => client_id,
    }) {
        Ok(_) => {
            // If the update was successful, return a success response
            json!({
                "status": "success"
            })
        }
        Err(e) => {
            // If an error occurs during the update, return a failure response with the error
            json!({
                "status": "failure",
                "error": e.to_string()
            })
        }
    }
}

pub fn update_access_rights(
    conn: &mut PooledConn,
    client_id: u64,
    image_id: &str,
    number_views: i32,
) -> serde_json::Value {
    if let Err(e) = conn.exec_drop(
        "INSERT INTO access_rights (client_id, image_id, number_views) VALUES (:client_id, :image_id, :number_views)",
        params! {
            "client_id" => client_id,
            "image_id" => image_id,
            "number_views" => number_views,
        },
    ) {
        return json!({
            "status": "failure",
            "error": e.to_string()
        });
    }

    json!({
        "status": "success"
    })
}



pub fn insert_into_resources(
    conn: &mut PooledConn,
    client_id: i32,
    image_id: &str,
) -> serde_json::Value {
    // Step 1: Insert a new entry into the `resources` table
    if let Err(e) = conn.exec_drop(
        "INSERT INTO resources (client_ID, image_ID) VALUES (:client_id, :image_id)",
        params! {
            "client_id" => client_id,
            "image_id" => image_id,
        },
    ) {
        // If an error occurs during the insert, return a failure response with the error
        return json!({
            "status": "failure",
            "error": e.to_string()
        });
    }

    // If the insert is successful, return a success response
    json!({
        "status": "success",
        "client_id": client_id,
        "image_id": image_id
    })
}


pub fn get_resources_by_client_id(conn: &mut PooledConn, client_id: u64) -> Value {
    // Step 1: Prepare the SQL query to fetch the data
    let query = "SELECT image_id, number_views FROM access_rights WHERE client_id = :client_id";

    // Step 2: Execute the query and collect the results
    match conn.exec_iter(query, params! { "client_id" => client_id }) {
        Ok(result) => {
            let mut resources = Vec::new();

            for row in result {
                // Parse each row into a tuple of (image_id, number_views)
                let (image_id, number_views): (String, i32) = mysql::from_row(row.unwrap());
                resources.push(json!({
                    "image_id": image_id,
                    "number_views": number_views,
                }));
            }

            // Return the results as JSON
            json!({
                "status": "success",
                "client_id": client_id,
                "resources": resources,
            })
        }
        Err(e) => {
            // Handle query execution errors
            json!({
                "status": "failure",
                "error": e.to_string(),
            })
        }
    }
}
