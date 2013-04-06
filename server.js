var http = require("http");
var fs = require('fs');
var path = require('path');
var url = require('url');

http.createServer(function(req,res) {
	var cacheLoad;

	if(req.url == "/" || req.url == "") {
		cacheLoad = "./www/index.html";
	}
	else {
		cacheLoad = "./www" + url.parse(req.url).pathname;
	}

	console.log("Caching: " + cacheLoad);

	var httpStatusCode = 200;

	fs.exists(cacheLoad,function(statusCheck) {

		if(!statusCheck) {
			httpStatusCode = 404;
			cacheLoad = "./404.html";
		}

		var cache = fs.readFileSync(cacheLoad);
		res.end(cache);
	});
}).listen(8000);